//! Pi coding agent, driven over ACP through the community `pi-acp` bridge.
//!
//! Pi itself speaks its own `--mode rpc` protocol, not ACP, so there is no native ACP
//! entrypoint to run. The `pi-acp` bridge (npm `pi-acp`) speaks ACP over stdio and spawns
//! `pi --mode rpc` underneath, which is how Zed drives Pi. This adapter is therefore
//! ACP-only, like Copilot's: a non-ACP run is refused instead of invoking a bridge that
//! expects ACP JSON-RPC with a bare prompt. Install both halves first:
//! `npm install -g @earendil-works/pi-coding-agent pi-acp`, then sign in with `pi /login`.
//! Pi owns its provider connections and sign-in; Craft Anything never sees them.
//!
//! Not yet run live against `pi-acp`: the ACP shape follows the bridge's documented
//! protocol (ACP JSON-RPC over stdio, no extra arguments), and the adapter, detection
//! and grants-table rows are covered by unit tests. Verify with a live run before
//! trusting a build to it.

use super::*;

pub struct Pi;

impl AgentAdapter for Pi {
    fn ids(&self) -> &'static [&'static str] {
        &["pi"]
    }

    fn args(&self, _cx: &ArgsCx) -> Vec<String> {
        // The native runner refuses `requires_acp` before it asks for arguments.
        Vec::new()
    }

    fn requires_acp(&self) -> bool {
        true
    }

    fn plain_fallback(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_is_acp_only_and_registered() {
        assert!(Pi.requires_acp(), "pi-acp speaks ACP, so every run of it is one");
        assert!(!Pi.plain_fallback());
        assert!(AgentAdapters::built_in().for_agent("pi").ids().contains(&"pi"), "registered, not the plain adapter");
        assert_eq!(acp::acp_args("pi"), Some([].as_slice()), "pi-acp needs no extra arguments");
    }
}
