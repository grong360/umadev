---
id: app-runtime-model-configurable
title: 应用运行时模型可配置（别硬编码开发底座的厂商）
domain: ai
category: 01-standards
difficulty: intermediate
tags: [llm, 运行时模型, 可配置, provider-abstraction, openai-compatible, dashscope, qwen, 大模型, 多厂商, env, 配置, 商业级]
quality_score: 95
last_updated: 2026-06-29
---

# 应用运行时模型可配置（别硬编码开发底座的厂商）

> 常见踩坑：用某个 AI 编码工具开发一个“会在运行时调用大模型”的应用时，生成的后端把运行时的 LLM 直接写死成开发工具自己用的那家（例如 Anthropic/Claude + `ANTHROPIC_API_KEY`）。用户想让交付的应用跑通义千问 / OpenAI / 本地模型，还得手工改后端。**开发用的工具底座，和应用运行时调用的模型，是两件完全不同的事，绝不能混为一谈。**

## 1. 核心原则

- **两层模型分离**：「开发期借用的大脑（写代码的工具）」≠「应用运行时调用的模型」。后者是业务选型，由需求/用户决定，不是由你用什么工具写代码决定。
- **运行时模型是配置项，不是常量**：模型 id、base URL、API Key 的环境变量名，三者都必须来自配置（环境变量 / 配置文件），代码里不出现写死的厂商端点或密钥。
- **默认值跟随需求**：需求里点名了运行时模型/厂商（如“运行时用千问 Max”“用 DashScope”“用 OpenAI”），就把默认配置指向它；没点名时，生成一个清晰可替换的 provider 占位层，并在交付物里说明“运行时模型可配置、怎么切换”。
- **绝不静默套用开发底座的厂商**：不要因为写代码时用的是某家工具，就把应用运行时也默认写成那家。

## 2. Provider 抽象层（最小骨架）

把对模型的调用收敛到一个薄抽象层，业务代码只依赖这个层，不直接耦合任意一家 SDK 的端点。

- 三元组来自配置：`model`（模型 id）、`base_url`（接入地址）、`api_key`（从指定环境变量读取）。
- 优先用 **OpenAI 兼容协议**的客户端：同一套 `base_url + api_key + model` 即可覆盖 OpenAI、DashScope/通义千问、DeepSeek、智谱 GLM、Moonshot/Kimi、本地 Ollama 等大多数主流厂商——换厂商是改配置，不是改代码。
- 不兼容 OpenAI 协议的厂商（个别国产/自研网关），在抽象层内部各写一个 adapter，对外暴露统一接口。

```bash
# .env.example —— 运行时模型可配置（默认值跟随需求；未点名则留可替换占位）
LLM_PROVIDER=openai-compatible
LLM_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1   # 例：通义千问 DashScope 兼容端点
LLM_MODEL=qwen-max                                               # 切换模型只改这一行
LLM_API_KEY_ENV=DASHSCOPE_API_KEY                                # 真正的密钥放在被引用的环境变量里
DASHSCOPE_API_KEY=                                               # 由部署方注入，不进版本库
```

```ts
// llm.ts —— 业务只调用这一层，端点/密钥/模型全部来自配置
import OpenAI from "openai"; // OpenAI 兼容客户端可对接多数厂商

const apiKeyEnv = process.env.LLM_API_KEY_ENV ?? "OPENAI_API_KEY";
const client = new OpenAI({
  baseURL: process.env.LLM_BASE_URL,          // 配置驱动，可指向千问/OpenAI/本地
  apiKey: process.env[apiKeyEnv],             // 从“被指定的环境变量名”读取真实密钥
});

export async function chat(messages: { role: string; content: string }[]) {
  return client.chat.completions.create({
    model: process.env.LLM_MODEL ?? "gpt-4o-mini", // 默认值跟随需求；切换只改配置
    messages,
  });
}
```

## 3. 落地清单（Checklist）

- [ ] 运行时 `model` / `base_url` / `api_key` 全部来自环境变量或配置文件，源码里没有写死的厂商端点或密钥。
- [ ] 需求点名了运行时厂商/模型 → 默认配置已指向它；未点名 → 留下清晰可替换的 provider 占位，且在 README/`.env.example` 写明如何切换。
- [ ] 对接走 provider 抽象层，业务代码不直接依赖某一家 SDK 的硬编码端点。
- [ ] 优先 OpenAI 兼容协议；非兼容厂商在抽象层内做 adapter，对外接口统一。
- [ ] 密钥只从环境变量注入，`.env` 不进版本库，仓库里只放 `.env.example` 占位。
- [ ] 交付物（README / 配置说明）明确写出“运行时模型可配置”，并给出切换到千问/OpenAI/本地模型的具体步骤。
- [ ] 提供超时、重试、错误兜底；切换厂商不需要改业务代码，只改配置即可生效。
- [ ] 不把开发所用工具底座的厂商（如 Anthropic/Claude）当成应用运行时的默认值。

## 4. 反模式（出现即不合格）

- 把应用运行时的 LLM 写死成开发工具自己用的那家（典型：默认 `ANTHROPIC_API_KEY` + Claude 端点），无视用户在需求里指定的运行时模型。
- 厂商端点 / 模型 id / 密钥硬编码在业务代码里，换模型要改源码、重新构建。
- 用户明确说“运行时用千问 / DeepSeek / 本地模型”，生成的代码却仍调另一家。
- 只支持单一厂商、没有 provider 抽象层，后续接第二家要大改。
- 把真实密钥写进源码或提交进版本库；没有 `.env.example` 占位与切换说明。
- 没在交付物里说明运行时模型可配置，用户只能逆向源码才知道怎么换。
