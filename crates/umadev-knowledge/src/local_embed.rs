//! Bundled local embedding backend — candle (pure Rust, no native ONNX/C++
//! runtime on CPU). Feature-gated behind `vector-local`.
//!
//! Loads a small bilingual BERT-family model (recommended:
//! `multilingual-e5-small`, 384-dim, zh+en) from a directory pointed to by
//! `UMADEV_EMBED_MODEL_DIR` and embeds text **fully offline** — no API key, no
//! network, no separate service. The model ships with the npm package (a
//! platform-independent `@umadev/model-e5-small` dir), so `npm i -g umadev` is
//! the only thing the user installs.
//!
//! **Fail-open by contract:** ANY problem (no model dir, missing files,
//! load/inference error) returns `None`, so the caller degrades to the HTTP
//! backend and then to BM25. The host is never blocked by the embedder.

use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokenizers::Tokenizer;

/// Env var pointing at the model directory (must hold `config.json`,
/// `model.safetensors`, `tokenizer.json`). Set by the npm `bin/cli.js` wrapper
/// to the bundled model path under `node_modules`.
const ENV_MODEL_DIR: &str = "UMADEV_EMBED_MODEL_DIR";

/// Whether a usable local model directory is configured and present on disk.
#[must_use]
pub fn is_available() -> bool {
    model_dir().is_some_and(|d| {
        d.join("tokenizer.json").is_file()
            && d.join("config.json").is_file()
            && d.join("model.safetensors").is_file()
    })
}

fn model_dir() -> Option<PathBuf> {
    // 1. Explicit override — set by the npm `bin/cli.js` wrapper to the bundled
    //    `@umacloud/model-e5-small` package path under `node_modules`.
    if let Some(d) = std::env::var(ENV_MODEL_DIR).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(d);
        if p.is_dir() {
            return Some(p);
        }
    }
    // 2. Conventional local location, auto-discovered with ZERO config: drop the
    //    three model files under `~/.umadev/embed-model` and the pure-Rust local
    //    vector track turns on — no env, no key, no network.
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .filter(|s| !s.is_empty())?;
    let p = PathBuf::from(home).join(".umadev").join("embed-model");
    p.is_dir().then_some(p)
}

/// The embedding width the bundled local model emits, read from its
/// `config.json` (`hidden_size`). Returns `None` when no usable local model is
/// configured or the config can't be read/parsed (fail-open).
///
/// [`crate::vector::active_dim`] consults this so the vector store + the
/// dim-invalidation guard track the LOCAL width (e5-small = 384) rather than
/// the HTTP-model default (1536) — see the H3 fix.
#[must_use]
pub fn local_dim() -> Option<usize> {
    // Minimal view of `config.json` — only the embedding width matters here.
    #[derive(serde::Deserialize)]
    struct HiddenSize {
        hidden_size: usize,
    }
    if !is_available() {
        return None;
    }
    let dir = model_dir()?;
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    let cfg: HiddenSize = serde_json::from_str(&text).ok()?;
    (cfg.hidden_size > 0).then_some(cfg.hidden_size)
}

/// A loaded model + tokenizer, cached process-wide so the ~220MB safetensors
/// load + BERT graph build + tokenizer parse happens ONCE, not on every query.
struct LoadedModel {
    model: BertModel,
    tokenizer: Tokenizer,
}

/// Process-wide model cache keyed by the resolved model directory. Loading is
/// multi-second work (read ~220MB safetensors, build the BERT graph, parse the
/// tokenizer); doing it per `embed_query` stalled every retrieval on the
/// default path. The cache loads once per dir (once, in production where the
/// dir is fixed by the npm wrapper). Fail-open: a load error is NOT cached, so
/// a later call can retry; a poisoned lock just falls back to a fresh load.
fn model_cache() -> &'static Mutex<HashMap<PathBuf, Arc<LoadedModel>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<LoadedModel>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fetch the cached model for `dir`, loading + caching it on first use. Returns
/// `None` (fail-open) on any load error, WITHOUT caching the failure so a
/// transient problem can be retried.
fn cached_model(dir: &Path) -> candle_core::Result<Arc<LoadedModel>> {
    // Fast path: already cached. The lock is held only for the map lookup, NOT
    // across the heavy load, so concurrent queries don't serialise behind it.
    if let Ok(map) = model_cache().lock() {
        if let Some(m) = map.get(dir) {
            return Ok(Arc::clone(m));
        }
    }
    // Slow path: load outside the lock. Two racing first-calls may both load;
    // last writer wins (both produce an equivalent model), which is rare and
    // far cheaper than holding the lock across a multi-second load.
    let loaded = Arc::new(load_model(dir)?);
    if let Ok(mut map) = model_cache().lock() {
        map.insert(dir.to_path_buf(), Arc::clone(&loaded));
    }
    Ok(loaded)
}

/// Read + build the model and tokenizer from `dir`. The expensive part that the
/// [`model_cache`] memoises.
fn load_model(dir: &Path) -> candle_core::Result<LoadedModel> {
    let device = Device::Cpu;
    let to_msg =
        |e: Box<dyn std::error::Error + Send + Sync>| candle_core::Error::Msg(e.to_string());

    let config_text = std::fs::read_to_string(dir.join("config.json"))
        .map_err(|e| candle_core::Error::Msg(e.to_string()))?;
    let config: Config =
        serde_json::from_str(&config_text).map_err(|e| candle_core::Error::Msg(e.to_string()))?;

    let weights = dir.join("model.safetensors");
    // Safe (non-mmap) load — the crate forbids `unsafe`: read the whole
    // safetensors file into tensors, then build the model.
    let tensors = candle_core::safetensors::load(&weights, &device)?;
    let vb = VarBuilder::from_tensors(tensors, DTYPE, &device);
    let model = BertModel::load(vb, &config)?;
    let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(to_msg)?;
    Ok(LoadedModel { model, tokenizer })
}

/// Embed `texts` with the bundled local model. `is_query` selects the e5
/// instruction prefix. Returns `None` (fail-open) on any error so the caller
/// can fall back to HTTP / BM25.
#[must_use]
pub fn embed_texts(texts: &[String], is_query: bool) -> Option<Vec<Vec<f32>>> {
    let dir = model_dir()?;
    match embed_inner(&dir, texts, is_query) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::debug!("local embed failed, falling back: {e}");
            None
        }
    }
}

fn embed_inner(dir: &Path, texts: &[String], is_query: bool) -> candle_core::Result<Vec<Vec<f32>>> {
    let device = Device::Cpu;
    let to_msg =
        |e: Box<dyn std::error::Error + Send + Sync>| candle_core::Error::Msg(e.to_string());

    // Reuse the cached (model, tokenizer) — loaded ONCE per process, not per
    // query. The heavy safetensors load + graph build happens on first use only.
    let loaded = cached_model(dir)?;
    let model = &loaded.model;
    let tokenizer = &loaded.tokenizer;

    let prefix = if is_query { "query: " } else { "passage: " };
    let mut out = Vec::with_capacity(texts.len());
    for t in texts {
        let enc = tokenizer
            .encode(format!("{prefix}{t}"), true)
            .map_err(to_msg)?;
        let ids = Tensor::new(enc.get_ids(), &device)?.unsqueeze(0)?;
        let type_ids = ids.zeros_like()?;
        let mask = Tensor::new(enc.get_attention_mask(), &device)?.unsqueeze(0)?;
        let hidden = model.forward(&ids, &type_ids, Some(&mask))?;
        let pooled = mean_pool(&hidden, &mask)?;
        let normed = l2_normalize(&pooled)?;
        out.push(normed.squeeze(0)?.to_vec1::<f32>()?);
    }
    Ok(out)
}

/// Attention-masked mean pooling over the token dimension. `hidden` is
/// `[1, n_tokens, dim]`, `mask` is `[1, n_tokens]`.
fn mean_pool(hidden: &Tensor, mask: &Tensor) -> candle_core::Result<Tensor> {
    let mask_f = mask.to_dtype(DTYPE)?.unsqueeze(2)?;
    let summed = hidden.broadcast_mul(&mask_f)?.sum(1)?;
    let counts = mask_f.sum(1)?;
    summed.broadcast_div(&counts)
}

/// L2-normalise each row of a `[1, dim]` tensor (cosine-ready).
fn l2_normalize(v: &Tensor) -> candle_core::Result<Tensor> {
    let norm = v.sqr()?.sum_keepdim(1)?.sqrt()?;
    v.broadcast_div(&norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dim_reads_hidden_size_and_drives_active_dim() {
        // H3: with a usable local model present, the REAL embedding width
        // (config.json `hidden_size`, e5-small = 384) must govern — both
        // local_dim() directly AND vector::active_dim() (which consults it),
        // so the store + dim-guard don't default to the 1536 HTTP-model width.
        // Hold the process-wide env lock so the ENV_MODEL_DIR / UMADEV_EMBED_DIM
        // mutations don't race the vector/index tests.
        let _env = crate::testsupport::env_guard();
        let prev = std::env::var(ENV_MODEL_DIR).ok();
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        // A minimal but parseable config carrying hidden_size; is_available()
        // only checks the three files EXIST, so empty siblings are fine.
        std::fs::write(dir.join("config.json"), r#"{"hidden_size": 384}"#).unwrap();
        std::fs::write(dir.join("tokenizer.json"), "{}").unwrap();
        std::fs::write(dir.join("model.safetensors"), b"").unwrap();

        std::env::set_var(ENV_MODEL_DIR, dir);
        std::env::remove_var("UMADEV_EMBED_DIM");
        std::env::remove_var("UMADEV_EMBED_MODEL");

        assert!(is_available(), "all three model files present");
        assert_eq!(local_dim(), Some(384), "hidden_size read from config.json");
        assert_eq!(
            crate::vector::active_dim(),
            384,
            "active_dim() must adopt the local backend's real width (H3)"
        );

        match prev {
            Some(v) => std::env::set_var(ENV_MODEL_DIR, v),
            None => std::env::remove_var(ENV_MODEL_DIR),
        }
    }

    #[test]
    fn local_dim_is_none_without_model_files() {
        // An existing dir that is MISSING the three model files => is_available()
        // is false => local_dim() is None (fail-open), so active_dim() falls back
        // to the model default. `without_local_model` points ENV_MODEL_DIR at an
        // empty dir (and holds the env lock), so this is deterministic regardless
        // of the machine's ~/.umadev fallback.
        let _no_local = crate::testsupport::without_local_model();
        assert!(!is_available());
        assert_eq!(local_dim(), None);
    }
}
