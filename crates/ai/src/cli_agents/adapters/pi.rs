//! Pi coding agent (`pi --mode json`).
//!
//! Pi runs one turn per invocation in JSONL event mode (`--mode json`), reading the prompt from
//! stdin so long prompts avoid OS command-line limits (#402). Models and thinking support are
//! discovered with `pi --offline --list-models`, and sessions resume with `--session <id>`.

use super::*;
use crate::Effort;
use std::collections::BTreeMap;

pub struct Pi;

fn pi_thinking(effort: Effort) -> &'static str {
    match effort {
        Effort::None => "off",
        Effort::Minimal => "minimal",
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
        Effort::Xhigh => "xhigh",
        Effort::Max => "max",
    }
}

fn has_flag(args: &[String], flag: &str, short: Option<&str>) -> bool {
    let prefix = format!("{flag}=");
    args.iter().any(|a| a == flag || a.starts_with(&prefix) || short.is_some_and(|s| a == s || a.starts_with(&format!("{s}="))))
}

/// Parse `pi --offline --list-models` tabular output into sorted, deduplicated models.
pub(crate) fn parse_list_models(bytes: &[u8]) -> Result<Vec<models::AgentModel>, ToolError> {
    if bytes.len() >= 4 * 1024 * 1024 {
        return Err(ToolError::new(ErrorCode::OverLimit, "Pi's model list is too large."));
    }
    let text = String::from_utf8_lossy(bytes);
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        let [provider, model, _, _, thinking_col, ..] = cols.as_slice() else {
            continue;
        };
        if provider.eq_ignore_ascii_case("provider") && model.eq_ignore_ascii_case("model") {
            continue;
        }
        if provider.is_empty() || model.is_empty() {
            continue;
        }
        let id = format!("{provider}/{model}");
        let thinking = if thinking_col.eq_ignore_ascii_case("yes") { Effort::CLI_ALL.to_vec() } else { Vec::new() };
        out.insert(id.clone(), models::AgentModel { id, name: format!("{model} ({provider})"), thinking });
    }
    Ok(out.into_values().collect())
}

impl AgentAdapter for Pi {
    fn ids(&self) -> &'static [&'static str] {
        &["pi"]
    }

    fn has_model_catalog(&self) -> bool {
        true
    }

    fn model_catalog(
        &self,
        config: &BackendConfig,
        options: &BackendOptions,
        directory: Option<&Path>,
        stop: &Stop,
    ) -> Result<Vec<models::AgentModel>, ToolError> {
        stop.check()?;
        let program = super::super::program_of(config, options)?;
        let raw_args = ["--offline".to_string(), "--list-models".to_string()];
        let mut proc = match config.ssh_host.as_deref().filter(|h| !h.trim().is_empty()) {
            Some(host) => {
                let mut remote = vec![program.clone()];
                remote.extend(raw_args);
                Process::new("ssh").args(crate::ssh::ssh_args(host, &remote))
            }
            None => {
                let mut p = self.process(&program).args(raw_args);
                if let Some(dir) = directory {
                    p = p.cwd(dir);
                }
                p
            }
        };
        let stop2 = stop.clone();
        proc = proc
            .timeout(Duration::from_secs(15))
            .idle_timeout(Duration::from_secs(10))
            .stdout_cap(4 * 1024 * 1024)
            .stderr_cap(64 * 1024)
            .cancel_when(move || stop2.is_stopped())
            .remediation("Check Pi on this computer (`pi --list-models`), then reload the models.");
        let out = proc.run()?;
        if !out.success {
            let name = Path::new(&program).file_name().map_or_else(|| program.clone(), |n| n.to_string_lossy().into_owned());
            return Err(craft_anything_platform::process::nonzero_error_with(&name, &out, None));
        }
        parse_list_models(&out.stdout)
    }

    fn validate_model(&self, config: &BackendConfig, options: &BackendOptions, directory: Option<&Path>, stop: &Stop) -> Result<(), ToolError> {
        stop.check()?;
        self.check_native_options(options, directory)?;
        if options.cli_model.is_none() && options.cli_thinking.is_none() {
            return Ok(());
        }
        models::validate(options, &self.model_catalog(config, options, directory, stop)?)
    }

    fn check_native_options(&self, options: &BackendOptions, _cwd: Option<&Path>) -> Result<(), ToolError> {
        if options.cli_model.is_some() && (has_flag(&options.args, "--model", None) || has_flag(&options.args, "--provider", None)) {
            return Err(ToolError::invalid("Choose Pi's model in its model setting, rather than in extra arguments."));
        }
        if options.cli_thinking.is_some() && has_flag(&options.args, "--thinking", None) {
            return Err(ToolError::invalid("Choose Pi's thinking depth in its thinking setting, rather than in extra arguments."));
        }
        Ok(())
    }

    fn args(&self, cx: &ArgsCx) -> Vec<String> {
        let mut a = vec!["--mode".to_string(), "json".to_string()];
        if cx.question {
            a.push("--no-session".into());
            if cx.reads_checked {
                a.push("--no-tools".into());
            } else {
                a.extend(["--tools".into(), "read,grep,find,ls".into()]);
            }
        } else if !cx.builder {
            if cx.reads_checked {
                a.push("--no-builtin-tools".into());
            } else {
                a.extend(["--tools".into(), "read,grep,find,ls".into()]);
            }
        }
        if let Some(m) = cx.options.cli_model.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            a.extend(["--model".into(), m.to_string()]);
        }
        if let Some(t) = cx.options.cli_thinking {
            a.extend(["--thinking".into(), pi_thinking(t).into()]);
        }
        if let Some(r) = cx.resume.filter(|_| !cx.question) {
            a.extend(["--session".into(), r.to_string()]);
        }
        a.extend(cx.options.args.iter().cloned());
        a
    }

    fn required_flags(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&["--help"], &["--mode", "--list-models"])
    }

    fn plain_fallback(&self) -> bool {
        false
    }

    fn streams_json(&self) -> bool {
        true
    }

    fn resumes(&self) -> bool {
        true
    }

    fn answers_questions(&self) -> bool {
        true
    }

    fn prompt_on_stdin(&self) -> bool {
        true
    }

    fn stdin_text(&self, preamble: &str, prompt: &str) -> String {
        format!("{preamble}{prompt}")
    }

    fn parse(&self, p: &mut StreamParser, v: &Value) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "session" => {
                if let Some(id) = v.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                    p.session_id = Some(id.to_string());
                    p.run_session = Some(id.to_string());
                }
            }
            "message_update" => {
                if let Some(ev) = v.get("assistantMessageEvent") {
                    match ev.get("type").and_then(Value::as_str).unwrap_or_default() {
                        "text_delta" => {
                            if let Some(delta) = ev.get("delta").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                                out.push(AgentEvent::TextDelta { text: delta.to_string() });
                            }
                        }
                        "thinking_start" => {
                            out.push(AgentEvent::Status { text: "Thinking…".into() });
                        }
                        _ => {}
                    }
                }
            }
            "tool_execution_start" => {
                let id = v.get("toolCallId").and_then(Value::as_str).unwrap_or_default();
                let name = v.get("toolName").and_then(Value::as_str).unwrap_or_default();
                let input = v.get("args").cloned().unwrap_or(Value::Null);
                if !id.is_empty() {
                    out.push(p.started(id, name, input));
                }
            }
            "tool_execution_end" => {
                let id = v.get("toolCallId").and_then(Value::as_str).unwrap_or_default();
                let name = v.get("toolName").and_then(Value::as_str).unwrap_or_default();
                if !id.is_empty() {
                    if !p.tool_names.contains_key(id) {
                        out.push(p.started(id, name, Value::Null));
                    }
                    let is_error = v.get("isError").and_then(Value::as_bool).unwrap_or(false);
                    let result = v.get("result").unwrap_or(&Value::Null);
                    let text = result.get("content").map(content_text).unwrap_or_else(|| content_text(result));
                    out.push(p.finished(id, !is_error, &text));
                }
            }
            "turn_end" => {
                if let Some(usage) = v.pointer("/message/usage") {
                    let count = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
                    p.input_tokens = p.input_tokens.saturating_add(count("input")).saturating_add(count("cacheRead")).saturating_add(count("cacheWrite"));
                    // Pi's `output` already includes `reasoning` tokens.
                    p.output_tokens = p.output_tokens.saturating_add(count("output"));
                    if !p.cost_incomplete {
                        p.cost_usd = usage.pointer("/cost/total").and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0).and_then(|cost| {
                            let total = p.cost_usd.unwrap_or(0.0) + cost;
                            total.is_finite().then_some(total)
                        });
                        if p.cost_usd.is_none() {
                            p.cost_incomplete = true;
                        }
                    }
                }
            }
            "auto_retry_start" => {
                let attempt = v.get("attempt").and_then(Value::as_u64).unwrap_or(1);
                let max = v.get("maxAttempts").and_then(Value::as_u64).unwrap_or(attempt);
                let why = v.get("errorMessage").and_then(Value::as_str).unwrap_or("retrying");
                out.push(AgentEvent::Status { text: format!("Pi retrying ({attempt}/{max}): {why}") });
            }
            "agent_end" => {
                if v.get("willRetry").and_then(Value::as_bool) != Some(true)
                    && let Some(last) = v
                        .get("messages")
                        .and_then(Value::as_array)
                        .and_then(|ms| ms.iter().rev().find(|m| m.get("role").and_then(Value::as_str) == Some("assistant")))
                {
                    let stop = last.get("stopReason").and_then(Value::as_str).unwrap_or_default();
                    if matches!(stop, "error" | "aborted") {
                        let why = last.get("errorMessage").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).unwrap_or(if stop == "aborted" {
                            "run aborted"
                        } else {
                            "run failed"
                        });
                        p.failure.get_or_insert_with(|| TurnFailure::new(FailureKind::Unknown));
                        out.push(AgentEvent::Error { message: format!("Pi: {why}"), remediation: None, log: None });
                    }
                }
            }
            "agent_settled" => {
                p.saw_result = true;
            }
            _ => {}
        }
        out
    }

    fn parse_stderr(&self, p: &mut StreamParser, line: &str) -> Vec<AgentEvent> {
        if line.to_ascii_lowercase().contains("no session found matching") {
            p.session_missing = true;
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(builder: bool, reads_checked: bool, question: bool, resume: Option<&str>, options: &BackendOptions) -> Vec<String> {
        let mcp = McpCommand { command: "craft-anything-cli".into(), args: vec!["mcp".into()], notes: None };
        Pi.args(&ArgsCx { options, prompt: "analyse repo", preamble: "PRE ", mcp: &mcp, resume, builder, reads_checked, question, message_file: None })
    }

    #[test]
    fn pi_args_cover_modes_models_and_sessions() {
        let default_opts = BackendOptions::default();
        assert_eq!(args(true, false, false, None, &default_opts), ["--mode", "json"]);
        assert_eq!(args(false, false, false, None, &default_opts), ["--mode", "json", "--tools", "read,grep,find,ls"]);
        assert_eq!(args(false, true, false, None, &default_opts), ["--mode", "json", "--no-builtin-tools"]);
        assert_eq!(args(false, false, true, Some("sess-1"), &default_opts), ["--mode", "json", "--no-session", "--tools", "read,grep,find,ls"]);
        assert_eq!(args(false, true, true, Some("sess-1"), &default_opts), ["--mode", "json", "--no-session", "--no-tools"]);

        let configured = BackendOptions {
            cli_model: Some("opencode-go/deepseek-v4.1-flash".into()),
            cli_thinking: Some(Effort::None),
            args: vec!["--no-extensions".into()],
            ..Default::default()
        };
        assert_eq!(
            args(true, false, false, Some("sess-1"), &configured),
            ["--mode", "json", "--model", "opencode-go/deepseek-v4.1-flash", "--thinking", "off", "--session", "sess-1", "--no-extensions"]
        );
        assert!(Pi.resumes() && Pi.answers_questions() && Pi.prompt_on_stdin() && Pi.has_model_catalog());
        assert_eq!(Pi.stdin_text("PRE ", "hi"), "PRE hi");
        assert!(AgentAdapters::built_in().for_agent("pi").ids().contains(&"pi"));
    }

    #[test]
    fn pi_rejects_conflicting_extra_model_and_thinking_flags() {
        let model_conflict = BackendOptions { cli_model: Some("p/m".into()), args: vec!["--model".into(), "other".into()], ..Default::default() };
        assert!(Pi.check_native_options(&model_conflict, None).is_err());

        let thinking_conflict = BackendOptions { cli_thinking: Some(Effort::High), args: vec!["--thinking=low".into()], ..Default::default() };
        assert!(Pi.check_native_options(&thinking_conflict, None).is_err());

        let clean =
            BackendOptions { cli_model: Some("p/m".into()), cli_thinking: Some(Effort::High), args: vec!["--no-extensions".into()], ..Default::default() };
        assert!(Pi.check_native_options(&clean, None).is_ok());
    }

    #[test]
    fn pi_list_models_parses_table_and_thinking() {
        let sample = b"provider     model               context  max-out  thinking  images\nopencode-go  deepseek-v4.1-flash 1M       384K     yes       yes\nlocal        plain-coder         32K      8K       no        no\n";
        let models = parse_list_models(sample).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "local/plain-coder");
        assert!(models[0].thinking.is_empty());
        assert_eq!(models[1].id, "opencode-go/deepseek-v4.1-flash");
        assert_eq!(models[1].thinking, Effort::CLI_ALL);
    }

    #[test]
    fn pi_stream_parsing_handles_session_deltas_tools_usage_and_errors() {
        let adapter: Arc<dyn AgentAdapter> = Arc::new(Pi);
        let mut p = StreamParser::new(adapter);

        assert!(p.line(r#"{"type":"session","version":3,"id":"sess-42","cwd":"/tmp"}"#).is_empty());
        assert_eq!(p.session_id.as_deref(), Some("sess-42"));

        let evs = p.line(r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello"}}"#);
        assert!(matches!(evs.as_slice(), [AgentEvent::TextDelta { text }] if text == "Hello"));

        let evs = p.line(r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"read","args":{"path":"Cargo.toml"}}"#);
        assert!(matches!(evs.as_slice(), [AgentEvent::ToolStarted { call_id, tool, .. }] if call_id == "call_1" && tool == "read"));

        let evs = p.line(r#"{"type":"tool_execution_end","toolCallId":"call_1","toolName":"read","result":{"content":[{"type":"text","text":"[package]"}]},"isError":false}"#);
        assert!(
            matches!(evs.as_slice(), [AgentEvent::ToolFinished { call_id, tool, ok: true, summary, .. }] if call_id == "call_1" && tool == "read" && summary == "[package]")
        );

        assert!(p.line(r#"{"type":"turn_end","message":{"role":"assistant","usage":{"input":100,"output":25,"cacheRead":20,"cacheWrite":5,"reasoning":10,"cost":{"total":0.015}}}}"#).is_empty());
        assert_eq!(p.input_tokens, 125);
        assert_eq!(p.output_tokens, 25);
        assert_eq!(p.cost_usd, Some(0.015));

        let evs = p.line(r#"{"type":"agent_end","willRetry":false,"messages":[{"role":"assistant","stopReason":"error","errorMessage":"401 unauthorized"}]}"#);
        assert!(matches!(evs.as_slice(), [AgentEvent::Error { message, .. }] if message == "Pi: 401 unauthorized"));
        assert!(p.failure.is_some());

        assert!(p.line(r#"{"type":"agent_settled","aborted":false}"#).is_empty());
        assert!(p.saw_result);

        assert!(Pi.parse_stderr(&mut p, "No session found matching 'sess-missing'").is_empty());
        assert!(p.session_missing);
    }
}
