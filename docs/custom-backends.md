# Gretongan Craft — generic AI backends and custom endpoints

> Fork note: this page documents what the Gretongan Craft fork adds on top of upstream
> [Craft Anything](https://github.com/storytold/craft-anything) (storytold, MIT OR Apache-2.0).
> Upstream already supported several backends; the gaps this fork closes are Pi, OpenClaw,
> and a custom endpoint whose API key can live in an environment variable. Binary and
> machine names stay `craft-anything-*`.

## What upstream already had (no change needed)

Claude Code is **not** the only backend upstream. The engine already has:

| Kind | How it runs |
| --- | --- |
| API backends | Anthropic, OpenAI, OpenRouter, Gemini, and any OpenAI-compatible HTTP server (`openai_compatible`) — Ollama, LM Studio, llama.cpp, vLLM, … |
| Agent CLIs | Claude Code, Codex, Copilot, Grok, Gemini CLI, OpenCode, Antigravity, Hermes, Maki, Aider, DeepSeek Harness (`dsh`), Devin, Qwen Code, Goose — run as subprocesses with our MCP server, some over ACP |
| Providers | Named rows in `crates/ai/src/providers.rs` (DeepSeek, Moonshot, xAI, Groq, Mistral, Together, Fireworks, DeepInfra) with their base URLs and key env vars |

DeepSeek was already covered twice: as an API provider (`https://api.deepseek.com/v1`,
`DEEPSEEK_API_KEY`) and as the DeepSeek Harness agent CLI (`dsh`, ACP via `dsh --profile acp`).

## What this fork adds

### 1. Pi — ACP-only, via the `pi-acp` bridge

Pi itself speaks its own `--mode rpc` protocol, **not** ACP. The community bridge `pi-acp`
(npm `pi-acp`) speaks ACP over stdio and spawns `pi --mode rpc` underneath — the same way
Zed drives Pi.

```sh
npm install -g @earendil-works/pi-coding-agent pi-acp
pi /login   # Pi owns its providers and sign-in; Craft Anything never sees them

craft-anything-cli ai add Pi --kind agent_cli --model pi --acp
craft-anything-cli ai detect   # finds pi-acp on PATH
```

Files: `crates/ai/src/cli_agents/adapters/pi.rs`, `ACP_AGENTS` (`"pi", &[]`),
`KNOWN` (`"pi", "pi-acp", "Pi (via pi-acp)"`), grants-table row (`Path::Acp`).
ACP-only like Copilot: a non-ACP run is refused, and it can't run over SSH.

**Status: not yet run live against `pi-acp`.** The wiring (adapter, detection, grants
row) is covered by unit tests; verify with a live run before trusting a build to it.

### 2. OpenClaw — ACP-only, via its own bridge

```sh
npm install -g openclaw
openclaw   # sign in / configure providers first; OpenClaw owns them

craft-anything-cli ai add OpenClaw --kind agent_cli --model openclaw --acp
```

`openclaw acp` exposes a Gateway session as an ACP server over stdio. Files:
`crates/ai/src/cli_agents/adapters/openclaw.rs`, `ACP_AGENTS` (`"openclaw", &["acp"]`),
`KNOWN`, grants-table row, plus a manual `setup` entry (`craft-anything-cli setup
--client openclaw` prints what to add to `~/.openclaw/openclaw.json`).

**Status: not yet run live against `openclaw acp`.** Same caveat as Pi.

### 3. Custom endpoint with a persistent key variable

`ai add --key-env VAR` used to read the variable **once** and store the key, then forget
which variable it came from. A backend added that way had no way to pick up a rotated key
from the environment, and a custom endpoint (one no provider row knows) had no env var at
all — its key could only live in the keychain.

Now the variable's **name** is saved on the backend (`BackendOptions.key_env`, the value
is never saved, logged or written anywhere), and key lookup is: stored key → `key_env`
variable → provider's variable → kind's variable. It applies to chats, builds, model
listing and reachability checks.

```sh
# Any OpenAI-compatible endpoint. --model skips the /v1/models probe; without it the
# server must answer GET <base>/v1/models with the key.
craft-anything-cli ai add MyRouter --kind openai_compatible \
  --base-url https://example.com/v1 --model some-model \
  --key-env MY_ROUTER_API_KEY
```

To switch to the variable after a key was stored: `craft-anything-cli ai key MyRouter --forget`
(a stored key still wins while it exists).

## 9Router as the endpoint (example)

Jun's self-hosted 9Router is an OpenAI-compatible server at
`https://vanguardvps.up.railway.app/v1`. Verified 2026-11-16: `GET /v1/models` without a
key answers `401 {"error":"API key required for remote API access"}` — the server is up
and speaks the OpenAI shape, it just needs a key. (Its model list is also reachable through
the 9Router connector's `models` command.)

```sh
export NINE_ROUTER_API_KEY='<your-9router-key>'   # placeholder — never paste a real key in chat or a file
craft-anything-cli ai add 9Router --kind openai_compatible \
  --base-url https://vanguardvps.up.railway.app/v1 \
  --model <a-model-id-from-its-/v1/models-list> \
  --key-env NINE_ROUTER_API_KEY
craft-anything-cli ai test 9Router
craft-anything-cli ai builder 9Router
```

Notes:

- The model id must be one the 9Router instance actually serves (list it first); a wrong
  id fails at call time, not at add time.
- 9Router routes to many upstream providers, so cost/quality depend on the model picked
  there, not on Craft Anything.
- The key is read from the environment on every run, so rotating it is just changing the
  variable (after `ai key 9Router --forget`, if a key was ever stored).

## Using it from Muse (John) — CLI and MCP, no desktop app

The MCP server is client-agnostic stdio JSON-RPC; any agent that can spawn a process can
drive it. There is nothing Claude-Code-specific in the CLI/MCP path.

```sh
# 1. Add a backend (any of the above), e.g. the 9Router one.
# 2. Run the MCP server (this is what a coding agent's MCP client spawns):
craft-anything-cli mcp --allow-run --project my-site.cany
# 3. Or drive it from the shell directly:
craft-anything-cli ai list
craft-anything-cli ai detect
craft-anything-cli rebuild https://example.com --help
craft-anything-cli doctor
```

`setup` registers the MCP server in known clients' config files (Claude Code, Codex,
Cursor, OpenCode, Hermes, Pi, OpenClaw via manual instructions, …). For any other client —
including Muse — the manual equivalent is: add a stdio MCP server named `craft-anything`
whose command is the path to `craft-anything-cli` with the argument `mcp`
(`mcp --allow-run` when the agent should be able to run build commands). The server
exposes the same tools the desktop app uses (`rebuild_start`, `ai_models_list`,
`ai_model_set`, …); the backend that builds is chosen with `ai builder <name>`, or per
project in Settings → AI → Roles.

## What is still blocked / untested

- **No Rust toolchain on this machine**, so the fork has not been compiled here:
  `cargo build -p craft-anything-cli` / `cargo test -p craft-anything-ai` must run on a
  machine with Rust before the new binary is trusted. The prebuilt v0.6.1 binary does not
  contain these changes.
- **Pi and OpenClaw have not been run live.** Only their wiring is tested. Their sign-in
  and provider setup live in the agents themselves.
- **Rebuilds need a working backend.** A rebuild through 9Router needs a valid key and a
  served model; a rebuild through Pi/OpenClaw needs the agent installed and signed in.
- **Santioni is out of scope for rebuilds**: it redirects automated browsers to
  `/unsupported`, and its assets are proprietary — study its layout/timing only, rebuild
  with your own assets.
