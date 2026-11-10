//! The verb glue: a parsed command line → the runner, in the live context
//! (this checkout, the host's HOME / PATH). Each returns the
//! exit code; the runners print as they go.

use std::path::{Path, PathBuf};

use humanify_ag_model::js::JsValue;

use super::Ctx;
use crate::committed::eval::{Pairs, pairs};
use crate::settings::{Parsed, run_config};

fn load_pairs(ctx: &Ctx) -> Result<Pairs, u8> {
    pairs(&ctx.repo).map(|l| l.value).map_err(|e| {
        ctx.say(e);
        2
    })
}

fn refuse(ctx: &Ctx, verb: &str, e: &str) -> u8 {
    ctx.say(format!("{verb}: {e}"));
    2
}

pub fn score(ctx: &Ctx, parsed: &Parsed, accepted: &[&str]) -> u8 {
    let Ok(pairs) = load_pairs(ctx) else { return 2 };
    match super::score::config_from(parsed, &pairs, ctx) {
        Ok(cfg) => super::score::run(ctx, &cfg, &run_config("eval score", accepted, parsed)),
        Err(e) => refuse(ctx, "eval score", &e),
    }
}

pub fn neutrality(ctx: &Ctx, parsed: &Parsed, accepted: &[&str]) -> u8 {
    let Ok(pairs) = load_pairs(ctx) else { return 2 };
    match super::neutrality::config_from(parsed, &pairs) {
        Ok(cfg) => {
            let mut rc = run_config("eval neutrality", accepted, parsed);
            if let JsValue::Object(o) = &mut rc {
                let resolved = super::records::obj(super::neutrality::resolved_record(&cfg));
                o.insert("resolved", resolved);
            }
            super::neutrality::run(ctx, &cfg, &pairs, &rc)
        }
        Err(e) => refuse(ctx, "eval neutrality", &e),
    }
}

pub fn walk(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let Ok(pairs) = load_pairs(ctx) else { return 2 };
    match super::walk::config_from(parsed, &pairs, &ctx.repo) {
        Ok(cfg) => super::walk::run(ctx, &cfg, &pairs),
        Err(e) => refuse(ctx, "eval walk", &e),
    }
}

pub fn walk_report(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let out = parsed.settings.text("walk.report_dir");
    super::walk::report_verb(ctx, &parsed.positionals, out.as_deref())
}

pub fn stub(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let Ok(pairs) = load_pairs(ctx) else { return 2 };
    let (bin, bundle, out, prior) = match parsed.positionals.as_slice() {
        [b, i, o] => (b, i, o, None),
        [b, i, o, p] => (b, i, o, Some(PathBuf::from(p))),
        _ => {
            return refuse(
                ctx,
                "run stub",
                "usage: run stub <bin> <bundle> <out> [<prior.js>]",
            );
        }
    };
    let sets = parsed.settings.list("run.pipeline_set").unwrap_or_default();
    if let Some(other) = sets.iter().find(|s| !s.starts_with("library.matching=")) {
        return refuse(
            ctx,
            "run stub",
            &format!(
                "--pipeline-set {other}: the stub run's config is fixed; only library.matching is taken"
            ),
        );
    }
    let index = match super::score::naming_mode_of(
        &sets,
        parsed
            .settings
            .text("eval.library_index")
            .map(|p| ctx.absolute(&p))
            .as_deref(),
        &pairs.library_index,
    ) {
        Ok((_, index)) => index,
        Err(e) => return refuse(ctx, "run stub", &e),
    };
    match super::stub::run(
        ctx,
        Path::new(bin),
        Path::new(bundle),
        Path::new(out),
        prior.as_deref(),
        index.as_deref(),
    ) {
        Ok(code) => u8::try_from(code).unwrap_or(1),
        Err(e) => refuse(ctx, "run stub", &e),
    }
}

pub fn index_build(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let s = &parsed.settings;
    let [spec] = parsed.positionals.as_slice() else {
        return refuse(
            ctx,
            "index build",
            "usage: index build <spec> --registry D --store D --index-work D --index-out D",
        );
    };
    let need = |k: &str, flag: &str| {
        s.text(k)
            .map(PathBuf::from)
            .ok_or(format!("{flag} <dir> is required"))
    };
    let build = (|| -> Result<super::index::IndexBuild, String> {
        Ok(super::index::IndexBuild {
            spec: PathBuf::from(spec),
            registry: need("index.registry", "--registry")?,
            store: need("index.store", "--store")?,
            work: need("index.work", "--index-work")?,
            out: need("index.out", "--index-out")?,
            chunks: s.int("index.chunks").unwrap_or(8) as u32,
            jobs: s.int("index.jobs").unwrap_or(4) as u32,
            merge_jobs: s.int("index.merge_jobs").unwrap_or(1) as u32,
        })
    })();
    match build {
        Ok(b) => super::index::run(ctx, &b),
        Err(e) => refuse(ctx, "index build", &e),
    }
}

pub fn boot_gate(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let [tree, version] = parsed.positionals.as_slice() else {
        return refuse(ctx, "boot-gate", "usage: boot-gate <tree> <version>");
    };
    let model = parsed
        .settings
        .text("boot.model")
        .unwrap_or_else(|| "claude-opus-4-1".into());
    super::boot::run_verb(ctx, Path::new(tree), version, &model)
}

pub fn build_bin_ref(ctx: &Ctx, parsed: &Parsed, refname: &str) -> (u8, String, String) {
    let work = parsed
        .settings
        .text("run.workdir")
        .unwrap_or_else(|| "/work".into());
    let builds = Path::new(&work).join("build-bin");
    match super::bin::build_ref(ctx, &ctx.repo, refname, &builds) {
        Ok(r) => (
            0,
            format!(
                "{{\"path\": {:?}, \"sha256\": {:?}, \"commit\": {:?}, \"dirty\": {}, \"name\": {:?}}}\n",
                r.path.display().to_string(),
                r.sha256,
                r.commit,
                r.dirty,
                r.name
            ),
            String::new(),
        ),
        Err(e) => (1, String::new(), format!("build-bin: {e}\n")),
    }
}

pub fn library_score(ctx: &Ctx, parsed: &Parsed) -> u8 {
    let s = &parsed.settings;
    let [label] = parsed.positionals.as_slice() else {
        return refuse(
            ctx,
            "library score",
            "usage: library score <label> --index <dir> --rule agreement|structural [flags]",
        );
    };
    let (Some(index), Some(rule)) = (s.text("libscore.index"), s.text("libscore.rule")) else {
        return refuse(
            ctx,
            "library score",
            "--index <dir> and --rule agreement|structural are required",
        );
    };
    let sets = s.list("run.pipeline_set").unwrap_or_default();
    if let Some(bad) = sets.iter().find(|e| !e.starts_with("library.")) {
        return refuse(
            ctx,
            "library score",
            &format!("--pipeline-set {bad}: the discover verb reads only [library.*] keys"),
        );
    }
    if let Err(e) = crate::settings::check_pipeline_sets(&sets, &["library.lookup.agreement"]) {
        return refuse(ctx, "library score", &e.join("\n"));
    }
    let c = super::library::LibraryScore {
        label: label.clone(),
        index: ctx.absolute(&index),
        rule,
        commit: s.text("libscore.commit"),
        jobs: s.int("libscore.jobs").unwrap_or(4) as usize,
        cpus: s.text("libscore.cpus").unwrap_or_else(|| "0-15".into()),
        corpus: s.text("libscore.corpus").unwrap_or_else(|| "dev".into()),
        sets: s.list("libscore.sets").filter(|v| !v.is_empty()),
        force_mixed: s.flag_on("run.force_mixed"),
        settings: sets,
        data_root: s.text("libscore.data_root"),
        registry: s
            .text("libscore.registry")
            .unwrap_or_else(|| crate::library::npm::REGISTRY.into())
            .into(),
    };
    match super::library::score_label(ctx, &c) {
        Ok(log) => {
            for l in log.lines() {
                ctx.say(l);
            }
            0
        }
        Err(e) => refuse(ctx, "library score", &e),
    }
}
