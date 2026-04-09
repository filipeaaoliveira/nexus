# Current Codex Assumptions

## Storage assumptions
- detector reads `~/.codex/state_5.sqlite`
- thread metadata comes from sqlite table `threads`
- subagent edges come from sqlite table `thread_spawn_edges`
- rollout events come from JSONL files referenced by `rollout_path`

## Runtime assumptions
- runtime is built by copying `auth.json` into a profile-specific `CODEX_HOME`
- sessions are shared via a symlinked `sessions` directory
- sqlite state is shared via `-c sqlite_home=...`
- repo runtime path is currently keyed as `current-repo`

## Quota assumptions
- current quota refresh uses the ChatGPT web API
- `codex status` text parsing exists as fallback/helper compatibility code
- rollout `token_count` parsing exists as fallback/helper compatibility code
- UI assumes exactly `5h` and `weekly`

## Risk summary
- sqlite schema drift breaks detection
- hardcoded repo key can cause cross-repo state collisions and session bleed
- unsupported quota sources drift from upstream
