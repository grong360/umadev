'use strict';

// Unit tests for two pure, network-free classifiers exported by the npm launcher:
//   1. invocationNeedsModel — which verbs trigger the ~224MB embedding-model fetch.
//   2. parseNodeMajor        — the ES5 shim's Node-version gate parser.
// Both are exercised through bin/cli.js (the ES5 shim), which re-exports the
// modern bin/cli-main.js surface plus its own gate helpers.

const assert = require('node:assert/strict');
const test = require('node:test');

const {
  invocationNeedsModel,
  NEEDS_MODEL,
  parseNodeMajor,
  MIN_NODE_MAJOR,
} = require('../umadev/bin/cli.js');

// Build a process.argv-shaped array: [node, script, ...rest].
function argv(...rest) {
  return ['node', 'cli.js'].concat(rest);
}

test('only retrieval verbs and the bare TUI fetch the model', () => {
  for (const verb of ['run', 'quick', 'redo', 'continue', 'revise']) {
    assert.equal(
      invocationNeedsModel(argv(verb, 'anything')),
      true,
      `${verb} retrieves knowledge → needs the model`,
    );
  }
  // Bare `umadev` with no verb launches the interactive TUI, which retrieves.
  assert.equal(invocationNeedsModel(argv()), true, 'bare TUI needs the model');
});

test('read-only, emergency, and utility verbs never block on the download', () => {
  // The exact verbs called out in the audit — a fresh install must not await a
  // 224MB fetch it never uses.
  const noModel = [
    'rollback',
    'verify',
    'deploy',
    'report',
    'spec',
    'history',
    'usage',
    'lessons',
    'memory',
    'doctor',
    'init',
    'pr',
    'ci',
    'install',
    'uninstall',
    'examples',
    'guide',
    'hook',
    'mcp',
    'skill',
    'knowledge-manage',
    'mcp-manage',
    'adopt',
    'update',
    '--version',
    '-V',
    '--help',
    '-h',
  ];
  for (const verb of noModel) {
    assert.equal(
      invocationNeedsModel(argv(verb)),
      false,
      `${verb} must start instantly (no model download)`,
    );
  }
});

test('a NEW/unknown verb defaults to no download (allow-set, not deny-list)', () => {
  assert.equal(invocationNeedsModel(argv('some-future-verb')), false);
  // The allow-set is exactly the five retrieval verbs.
  assert.deepEqual(
    [...NEEDS_MODEL].sort(),
    ['continue', 'quick', 'redo', 'revise', 'run'],
  );
});

test('the ES5 shim Node-version gate parses the major correctly', () => {
  assert.equal(parseNodeMajor('v18.17.0'), 18);
  assert.equal(parseNodeMajor('20.1.2'), 20);
  assert.equal(parseNodeMajor('v8.9.4'), 8);
  assert.equal(parseNodeMajor(''), 0, 'empty → 0 (fails the floor safely)');
  assert.equal(parseNodeMajor(null), 0, 'null → 0');
  assert.equal(parseNodeMajor('garbage'), 0, 'garbage → 0');
  assert.equal(MIN_NODE_MAJOR, 18, 'gate floor matches package.json engines');
  // The gate would reject an ancient runtime and accept a supported one.
  assert.ok(parseNodeMajor('v8.0.0') < MIN_NODE_MAJOR, 'Node 8 is rejected');
  assert.ok(parseNodeMajor('v18.0.0') >= MIN_NODE_MAJOR, 'Node 18 is accepted');
});
