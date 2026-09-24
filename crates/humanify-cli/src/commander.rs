//! A commander-13 command surface, ported for the pipeline's own program.
//!
//! TS original: the `commander` npm package (13.1.0, `lib/command.js`,
//! `lib/option.js`, `lib/help.js`) as the TS pipeline configures it
//! (`src/cli.ts`: `showHelpAfterError(true)`, no suggestions;
//! `src/index.ts`: version + `enablePositionalOptions`). The harness spawns
//! the pipeline with commander's argv grammar (contract 14 §1, §7), so the
//! grammar IS contract surface — including its accidents:
//!
//! - `--skip-libraries, --no-skip-libraries` is ONE option whose "short"
//!   flag is `--skip-libraries` (commander's two-long-flags form) and whose
//!   long flag is negated, so BOTH spellings set `skipLibraries = false`
//!   (TS finding, recorded in the WPB.4 hand-back);
//! - a required option takes the next argv entry even when it starts with
//!   `-` (`--model --split` sets model to "--split");
//! - repeated options are last-wins; `-vv` counts; `-c5` attaches;
//! - usage errors print `error: ...`, a blank line and the command's help to
//!   stderr and exit 1 (never clap's 2 — contract §2 keeps exit 2 out of the
//!   pipeline).
//!
//! clap cannot express these (it would need an override at every point), so
//! the pipeline surface is this port, while the migration verbs keep clap.
//! The surface gate (`surface_test.rs`) replays test/parity/wpb4-cli-surface.json,
//! recorded from the REAL commander program, through this parser.

use serde_json::Value;

/// Where an option's value came from (commander's `getOptionValueSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSource {
    Default,
    Cli,
}

impl ValueSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueSource::Default => "default",
            ValueSource::Cli => "cli",
        }
    }
}

/// How an option processes a value — the one custom `parseArg` the
/// pipeline declares is the `-v` counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgParser {
    None,
    /// `(_, prev) => (prev || 0) + 1`.
    Increment,
}

/// One commander `Option`, constructed from its flags string exactly like
/// `new Option(flags, description)`.
#[derive(Clone, Debug)]
pub struct CliOption {
    pub flags: String,
    pub description: String,
    pub short: Option<String>,
    pub long: Option<String>,
    pub negate: bool,
    pub required: bool,
    pub optional: bool,
    pub variadic: bool,
    pub default_value: Option<Value>,
    pub parser: ArgParser,
    /// Rust-only migration flags are hidden from help and named in the
    /// surface gate's exception list (never silently extra).
    pub hidden: bool,
}

impl CliOption {
    pub fn new(flags: &str, description: &str) -> CliOption {
        let (short, long) = split_option_flags(flags);
        let negate = long.as_deref().is_some_and(|l| l.starts_with("--no-"));
        CliOption {
            flags: flags.to_string(),
            description: description.to_string(),
            short,
            long,
            negate,
            required: flags.contains('<'),
            optional: flags.contains('['),
            variadic: is_variadic(flags),
            default_value: None,
            parser: ArgParser::None,
            hidden: false,
        }
    }

    /// `option.name()`: the long flag without `--`, else the short without `-`.
    pub fn name(&self) -> String {
        match (&self.long, &self.short) {
            (Some(l), _) => l.trim_start_matches("--").to_string(),
            (None, Some(s)) => s.trim_start_matches('-').to_string(),
            (None, None) => String::new(),
        }
    }

    /// `option.attributeName()`: camelCase, with a negated `no-` dropped.
    pub fn attribute_name(&self) -> String {
        let name = self.name();
        if self.negate {
            camelcase(name.strip_prefix("no-").unwrap_or(&name))
        } else {
            camelcase(&name)
        }
    }

    /// `option.is(arg)`.
    pub fn is(&self, arg: &str) -> bool {
        self.short.as_deref() == Some(arg) || self.long.as_deref() == Some(arg)
    }

    pub fn is_boolean(&self) -> bool {
        !self.required && !self.optional && !self.negate
    }
}

/// `/\w\.\.\.[>\]]$/` — a variadic option.
fn is_variadic(flags: &str) -> bool {
    let b = flags.as_bytes();
    let n = b.len();
    n >= 5
        && matches!(b[n - 1], b'>' | b']')
        && &b[n - 4..n - 1] == b"..."
        && (b[n - 5].is_ascii_alphanumeric() || b[n - 5] == b'_')
}

/// commander's `camelcase`: `a-b-c` → `aBC`-style (each later word's first
/// character upper-cased).
fn camelcase(s: &str) -> String {
    let mut out = String::new();
    for (i, word) in s.split('-').enumerate() {
        if i == 0 {
            out.push_str(word);
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

/// `splitOptionFlags`: short and/or long; two long flags make the first the
/// "short" one (`--ws, --workspace`). The flags here are the pipeline's own
/// literals, so the TS's unsupported-format throws are unreachable and a
/// malformed literal is a programming error.
fn split_option_flags(flags: &str) -> (Option<String>, Option<String>) {
    let is_short = |p: &str| {
        let mut c = p.chars();
        c.next() == Some('-') && matches!(c.next(), Some(x) if x != '-') && c.next().is_none()
    };
    let is_long = |p: &str| p.starts_with("--") && p.len() > 2 && !p[2..].starts_with('-');
    let mut parts: Vec<&str> = flags
        .split([' ', '|', ','])
        .filter(|p| !p.is_empty())
        .collect();
    parts.push("guard");
    let mut short: Option<String> = None;
    let mut long: Option<String> = None;
    if is_short(parts[0]) {
        short = Some(parts.remove(0).to_string());
    }
    if is_long(parts[0]) {
        long = Some(parts.remove(0).to_string());
    }
    if short.is_none() && is_short(parts[0]) {
        short = Some(parts.remove(0).to_string());
    }
    if short.is_none() && is_long(parts[0]) {
        short = long.take();
        long = Some(parts.remove(0).to_string());
    }
    assert!(
        !parts[0].starts_with('-') && (short.is_some() || long.is_some()),
        "option creation failed for flags '{flags}'"
    );
    (short, long)
}

/// A declared positional argument (`<input>`).
#[derive(Clone, Debug)]
pub struct CliArgument {
    pub name: String,
    pub required: bool,
    pub variadic: bool,
    pub description: String,
}

impl CliArgument {
    pub fn new(spec: &str, description: &str) -> CliArgument {
        let required = spec.starts_with('<');
        let inner = spec
            .trim_start_matches(['<', '['])
            .trim_end_matches(['>', ']']);
        let variadic = inner.ends_with("...");
        CliArgument {
            name: inner.trim_end_matches("...").to_string(),
            required,
            variadic,
            description: description.to_string(),
        }
    }

    /// `humanReadableArgName`.
    fn human_readable(&self) -> String {
        let name = format!("{}{}", self.name, if self.variadic { "..." } else { "" });
        if self.required {
            format!("<{name}>")
        } else {
            format!("[{name}]")
        }
    }
}

/// One command's option values, in commander's `_optionValues` insertion
/// order, each with its source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionValues {
    entries: Vec<(String, Value, ValueSource)>,
}

impl OptionValues {
    fn set(&mut self, key: &str, value: Value, source: ValueSource) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.0 == key) {
            e.1 = value;
            e.2 = source;
        } else {
            self.entries.push((key.to_string(), value, source));
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|e| e.0 == key).map(|e| &e.1)
    }

    pub fn source(&self, key: &str) -> Option<ValueSource> {
        self.entries.iter().find(|e| e.0 == key).map(|e| e.2)
    }

    /// A string-valued option (every `<value>` option).
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// A boolean option; `None` when never set (no default).
    pub fn bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }

    pub fn entries(&self) -> &[(String, Value, ValueSource)] {
        &self.entries
    }
}

/// A commander `Command` as the pipeline builds it.
#[derive(Clone, Debug)]
pub struct CliCommand {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub arguments: Vec<CliArgument>,
    /// Declared options, in declaration order (the version option first
    /// when `.version()` was called before the others, as in index.ts).
    pub options: Vec<CliOption>,
    pub commands: Vec<CliCommand>,
    pub enable_positional_options: bool,
    /// Commander's `_optionValues` after construction (declared defaults).
    defaults: OptionValues,
}

/// Where a parse ended.
#[derive(Clone, Debug, PartialEq)]
pub enum ParseOutcome {
    /// An action is to run: the command path's leaf name ("pipeline" for the
    /// root), its processed arguments, and its option values.
    Action {
        command: String,
        args: Vec<String>,
        opts: OptionValues,
    },
    /// Commander exited during the parse (help, version, usage error).
    Exit {
        exit_code: i32,
        code: &'static str,
        stdout: String,
        stderr: String,
    },
}

impl CliCommand {
    pub fn new(name: &str) -> CliCommand {
        CliCommand {
            name: name.to_string(),
            description: String::new(),
            version: None,
            arguments: Vec::new(),
            options: Vec::new(),
            commands: Vec::new(),
            enable_positional_options: false,
            defaults: OptionValues::default(),
        }
    }

    pub fn description(mut self, d: &str) -> Self {
        self.description = d.to_string();
        self
    }

    /// `.version(str)`: registers `-V, --version` (no default value).
    pub fn version(mut self, v: &str) -> Self {
        self.version = Some(v.to_string());
        self.options
            .push(CliOption::new("-V, --version", "output the version number"));
        self
    }

    pub fn enable_positional_options(mut self) -> Self {
        self.enable_positional_options = true;
        self
    }

    pub fn argument(mut self, spec: &str, description: &str) -> Self {
        self.arguments.push(CliArgument::new(spec, description));
        self
    }

    /// `.option(flags, description[, defaultValue])`.
    pub fn option(self, flags: &str, description: &str, default: Option<Value>) -> Self {
        let mut o = CliOption::new(flags, description);
        o.default_value = default;
        self.add_option(o)
    }

    /// `.option(flags, description, fn, defaultValue)` for the counter.
    pub fn counter_option(self, flags: &str, description: &str, default: i64) -> Self {
        let mut o = CliOption::new(flags, description);
        o.default_value = Some(Value::from(default));
        o.parser = ArgParser::Increment;
        self.add_option(o)
    }

    /// `addOption`: register, then store the default — a `--no-*` option
    /// defaults its attribute to true unless a flag equal to the positive
    /// long form is ALREADY registered (itself included: `_findOption` runs
    /// after the push, which is why `--skip-libraries, --no-skip-libraries`
    /// has no default).
    pub fn add_option(mut self, option: CliOption) -> Self {
        let attr = option.attribute_name();
        self.options.push(option.clone());
        if option.negate {
            let positive = option
                .long
                .as_deref()
                .unwrap_or_default()
                .replacen("--no-", "--", 1);
            if !self.options.iter().any(|o| o.is(&positive)) {
                let v = option.default_value.clone().unwrap_or(Value::Bool(true));
                self.defaults.set(&attr, v, ValueSource::Default);
            }
        } else if let Some(v) = option.default_value.clone() {
            self.defaults.set(&attr, v, ValueSource::Default);
        }
        self
    }

    pub fn subcommand(mut self, cmd: CliCommand) -> Self {
        let mut cmd = cmd;
        // copyInheritedSettings, the parts the parse reads.
        cmd.enable_positional_options = self.enable_positional_options;
        self.commands.push(cmd);
        self
    }

    fn find_option(&self, arg: &str) -> Option<&CliOption> {
        self.options.iter().find(|o| o.is(arg))
    }

    fn find_command(&self, name: &str) -> Option<&CliCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    /// `program.parse(argv)` over the user args (no node/script prefix).
    pub fn parse(&self, argv: &[String]) -> ParseOutcome {
        self.parse_command(&[], argv, &[], "pipeline")
    }

    /// `_parseCommand(operands, unknown)`. `ancestors` names the parents
    /// for the usage line; `action_name` is what an Action reports.
    fn parse_command(
        &self,
        operands: &[String],
        unknown: &[String],
        ancestors: &[&str],
        action_name: &str,
    ) -> ParseOutcome {
        let mut values = self.defaults.clone();
        let parsed = match self.parse_options(unknown, &mut values, ancestors) {
            Ok(p) => p,
            Err(exit) => return exit,
        };
        let mut operands: Vec<String> = operands.to_vec();
        operands.extend(parsed.operands.iter().cloned());
        let unknown = parsed.unknown.clone();
        let mut args = operands.clone();
        args.extend(unknown.iter().cloned());

        if let Some(first) = operands.first()
            && let Some(sub) = self.find_command(first)
        {
            let mut chain: Vec<&str> = ancestors.to_vec();
            chain.push(&self.name);
            return sub.parse_command(&operands[1..], &unknown, &chain, &sub.name);
        }
        // _outputHelpIfRequested(parsed.unknown)
        if parsed.unknown.iter().any(|a| a == "-h" || a == "--help") {
            return ParseOutcome::Exit {
                exit_code: 0,
                code: "commander.helpDisplayed",
                stdout: self.help_information(ancestors),
                stderr: String::new(),
            };
        }
        // Every pipeline command has an action handler.
        if let Some(flag) = parsed.unknown.first() {
            return self.error(
                ancestors,
                &format!("error: unknown option '{flag}'"),
                "commander.unknownOption",
            );
        }
        // _processArguments → _checkNumberOfArguments
        for (i, a) in self.arguments.iter().enumerate() {
            if a.required && args.get(i).is_none() {
                return self.error(
                    ancestors,
                    &format!("error: missing required argument '{}'", a.name),
                    "commander.missingArgument",
                );
            }
        }
        let variadic_last = self.arguments.last().is_some_and(|a| a.variadic);
        if !self.arguments.is_empty() && !variadic_last && args.len() > self.arguments.len() {
            let expected = self.arguments.len();
            let s = if expected == 1 { "" } else { "s" };
            let for_sub = if ancestors.is_empty() {
                String::new()
            } else {
                format!(" for '{}'", self.name)
            };
            return self.error(
                ancestors,
                &format!(
                    "error: too many arguments{for_sub}. Expected {expected} argument{s} but got {}.",
                    args.len()
                ),
                "commander.excessArguments",
            );
        }
        ParseOutcome::Action {
            command: action_name.to_string(),
            args: args.into_iter().take(self.arguments.len()).collect(),
            opts: values,
        }
    }

    /// `parseOptions(argv)`: assigns recognised options, splits operands
    /// from the unknown tail. Version and missing-value exits happen here,
    /// in argv order, exactly as commander's listeners fire them.
    fn parse_options(
        &self,
        argv: &[String],
        values: &mut OptionValues,
        ancestors: &[&str],
    ) -> Result<Parsed, ParseOutcome> {
        let maybe_option = |a: &str| a.len() > 1 && a.starts_with('-');
        let mut operands: Vec<String> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        let mut dest_unknown = false;
        let mut args: std::collections::VecDeque<String> = argv.iter().cloned().collect();
        while let Some(arg) = args.pop_front() {
            if arg == "--" {
                let dest = if dest_unknown {
                    &mut unknown
                } else {
                    &mut operands
                };
                if dest_unknown {
                    dest.push(arg);
                }
                dest.extend(args.drain(..));
                break;
            }
            if maybe_option(&arg)
                && let Some(option) = self.find_option(&arg)
            {
                if option.optional {
                    // historical: the value is the next arg unless an option
                    let value = match args.front() {
                        Some(next) if !maybe_option(next) => args.pop_front(),
                        _ => None,
                    };
                    self.emit_option(option, value, values)?;
                } else if option.required {
                    let Some(value) = args.pop_front() else {
                        return Err(self.error(
                            ancestors,
                            &format!("error: option '{}' argument missing", option.flags),
                            "commander.optionMissingArgument",
                        ));
                    };
                    self.emit_option(option, Some(value), values)?;
                } else {
                    self.emit_option(option, None, values)?;
                }
                continue;
            }
            if self.try_attached_value(&arg, &mut args, values)? {
                continue;
            }
            if maybe_option(&arg) {
                dest_unknown = true;
            }
            if self.enable_positional_options
                && operands.is_empty()
                && unknown.is_empty()
                && self.find_command(&arg).is_some()
            {
                operands.push(arg);
                unknown.extend(args.drain(..));
                break;
            }
            if dest_unknown {
                unknown.push(arg);
            } else {
                operands.push(arg);
            }
        }
        Ok(Parsed { operands, unknown })
    }

    /// The two attached-value forms, in commander's order: a combo after a
    /// single dash (`-vv`, `-c5` — a boolean emits and puts the rest back)
    /// and a known long flag with `=value`. True when `arg` was consumed.
    fn try_attached_value(
        &self,
        arg: &str,
        args: &mut std::collections::VecDeque<String>,
        values: &mut OptionValues,
    ) -> Result<bool, ParseOutcome> {
        if arg.len() > 2 && arg.starts_with('-') && !arg[1..].starts_with('-') {
            let first: String = arg.chars().take(2).collect();
            if let Some(option) = self.find_option(&first) {
                let rest = &arg[first.len()..];
                if option.required || option.optional {
                    self.emit_option(option, Some(rest.to_string()), values)?;
                } else {
                    self.emit_option(option, None, values)?;
                    args.push_front(format!("-{rest}"));
                }
                return Ok(true);
            }
        }
        if arg.starts_with("--")
            && let Some(eq) = arg.find('=')
            && eq > 2
            && let Some(option) = self.find_option(&arg[..eq])
            && (option.required || option.optional)
        {
            self.emit_option(option, Some(arg[eq + 1..].to_string()), values)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// The `option:<name>` listener: the version option exits; everything
    /// else goes through `handleOptionValue`.
    fn emit_option(
        &self,
        option: &CliOption,
        value: Option<String>,
        values: &mut OptionValues,
    ) -> Result<(), ParseOutcome> {
        if option.long.as_deref() == Some("--version")
            && let Some(v) = &self.version
        {
            return Err(ParseOutcome::Exit {
                exit_code: 0,
                code: "commander.version",
                stdout: format!("{v}\n"),
                stderr: String::new(),
            });
        }
        let attr = option.attribute_name();
        let v = match (option.parser, value) {
            (ArgParser::Increment, _) => {
                let prev = values.get(&attr).and_then(Value::as_i64).unwrap_or(0);
                Value::from(prev + 1)
            }
            (ArgParser::None, Some(s)) => Value::String(s),
            (ArgParser::None, None) => Value::Bool(!option.negate),
        };
        values.set(&attr, v, ValueSource::Cli);
        Ok(())
    }

    /// `this.error(message)` under `showHelpAfterError(true)`.
    fn error(&self, ancestors: &[&str], message: &str, code: &'static str) -> ParseOutcome {
        ParseOutcome::Exit {
            exit_code: 1,
            code,
            stdout: String::new(),
            stderr: format!("{message}\n\n{}", self.help_information(ancestors)),
        }
    }

    /// `usage()`: `[options]` (the help option always exists), `[command]`,
    /// then the arguments.
    fn usage(&self) -> String {
        let mut parts = vec!["[options]".to_string()];
        if !self.commands.is_empty() {
            parts.push("[command]".to_string());
        }
        parts.extend(self.arguments.iter().map(CliArgument::human_readable));
        parts.join(" ")
    }

    /// The help option (`-h, --help`) in the visible options list.
    fn visible_options(&self) -> Vec<CliOption> {
        let mut out: Vec<CliOption> = self.options.iter().filter(|o| !o.hidden).cloned().collect();
        out.push(CliOption::new("-h, --help", "display help for command"));
        out
    }

    /// `helpInformation()` at helpWidth 80 without colors — the non-TTY
    /// rendering the harness and the probe see (a TTY's wider helpWidth
    /// only changes where long descriptions wrap; not contract, 14 §3).
    pub fn help_information(&self, ancestors: &[&str]) -> String {
        let help_width = 80usize;
        let options = self.visible_options();
        let sub_terms: Vec<String> = self.commands.iter().map(subcommand_term).collect();
        let term_width = options
            .iter()
            .map(|o| o.flags.chars().count())
            .chain(sub_terms.iter().map(|t| t.chars().count()))
            .chain(self.arguments.iter().map(|a| a.name.chars().count()))
            .max()
            .unwrap_or(0);
        let mut prefix = String::new();
        for a in ancestors {
            prefix.push_str(a);
            prefix.push(' ');
        }
        let mut out: Vec<String> = vec![
            format!("Usage: {prefix}{} {}", self.name, self.usage()),
            String::new(),
        ];
        if !self.description.is_empty() {
            out.push(box_wrap(&self.description, help_width));
            out.push(String::new());
        }
        if self.arguments.iter().any(|a| !a.description.is_empty()) {
            out.push("Arguments:".to_string());
            for a in &self.arguments {
                out.push(format_item(&a.name, term_width, &a.description, help_width));
            }
            out.push(String::new());
        }
        out.push("Options:".to_string());
        for o in &options {
            out.push(format_item(
                &o.flags,
                term_width,
                &option_description(o),
                help_width,
            ));
        }
        out.push(String::new());
        if !self.commands.is_empty() {
            out.push("Commands:".to_string());
            for (c, term) in self.commands.iter().zip(&sub_terms) {
                out.push(format_item(term, term_width, &c.description, help_width));
            }
            out.push(String::new());
        }
        out.join("\n")
    }
}

struct Parsed {
    operands: Vec<String>,
    unknown: Vec<String>,
}

/// `subcommandTerm`.
fn subcommand_term(cmd: &CliCommand) -> String {
    let args: Vec<String> = cmd
        .arguments
        .iter()
        .map(CliArgument::human_readable)
        .collect();
    let mut t = cmd.name.clone();
    if !cmd.options.is_empty() {
        t.push_str(" [options]");
    }
    if !args.is_empty() {
        t.push(' ');
        t.push_str(&args.join(" "));
    }
    t
}

/// `optionDescription`: the description plus `(default: <JSON>)` for value
/// options and boolean-typed defaults of boolean options.
fn option_description(o: &CliOption) -> String {
    let mut extra: Vec<String> = Vec::new();
    if let Some(d) = &o.default_value {
        let show = o.required || o.optional || (o.is_boolean() && d.is_boolean());
        if show {
            let js: humanify_model::js::JsValue =
                serde_json::from_value(d.clone()).expect("a default is plain JSON");
            extra.push(format!("default: {}", humanify_model::js::stringify(&js)));
        }
    }
    if extra.is_empty() {
        o.description.clone()
    } else {
        format!("{} ({})", o.description, extra.join(", "))
    }
}

/// `formatItem`: indent 2, the term padded to `term_width`, two spaces,
/// the description box-wrapped when at least `minWidthToWrap` (40) columns
/// remain.
fn format_item(term: &str, term_width: usize, description: &str, help_width: usize) -> String {
    if description.is_empty() {
        return format!("  {term}");
    }
    let padded = format!("{term:<term_width$}");
    let remaining = help_width as isize - term_width as isize - 4;
    let preformatted = description
        .split('\n')
        .skip(1)
        .any(|l| l.starts_with(|c: char| c.is_whitespace() && c != '\r' && c != '\n'));
    let formatted = if remaining < 40 || preformatted {
        description.to_string()
    } else {
        box_wrap(description, remaining as usize)
            .replace('\n', &format!("\n{}", " ".repeat(term_width + 2)))
    };
    format!("  {padded}  {}", formatted.replace('\n', "\n  "))
}

/// `boxWrap`: greedy wrap over `/[\s]*[^\s]+/g` chunks.
fn box_wrap(text: &str, width: usize) -> String {
    if width < 40 {
        return text.to_string();
    }
    let mut wrapped: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let chunks = whitespace_chunks(line);
        if chunks.is_empty() {
            wrapped.push(String::new());
            continue;
        }
        let mut sum = chunks[0].clone();
        let mut sum_width = sum.chars().count();
        for chunk in &chunks[1..] {
            let w = chunk.chars().count();
            if sum_width + w <= width {
                sum.push_str(chunk);
                sum_width += w;
                continue;
            }
            wrapped.push(std::mem::take(&mut sum));
            let next = chunk.trim_start();
            sum = next.to_string();
            sum_width = next.chars().count();
        }
        wrapped.push(sum);
    }
    wrapped.join("\n")
}

/// `line.match(/[\s]*[^\s]+/g)` — trailing whitespace is dropped.
fn whitespace_chunks(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    for c in line.chars() {
        let ws = humanify_model::js::is_js_whitespace(c);
        if ws && in_word {
            out.push(std::mem::take(&mut cur));
            in_word = false;
        }
        cur.push(c);
        if !ws {
            in_word = true;
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}
