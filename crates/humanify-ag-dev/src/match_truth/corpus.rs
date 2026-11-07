//! The pinned corpus (exp092; ported from experiments/lib/match-truth/corpus.ts):
//! REAL npm packages, two published versions each, matched against the
//! package's OWN shipped single-file build (the `file` path inside both
//! tarballs — no rebuild, so nothing the harness does can perturb the
//! ground truth; what changed between the versions is whatever the
//! package's own toolchain shipped, which is exactly the pipeline's real
//! input regime).
//!
//! Chosen for diversity (utilities, dates, promises, networking, DOM, data
//! structures), a CJS-or-UMD single-file build present at the SAME path in
//! both versions, and version pairs one realistic release distance apart.
//! Pinning is exact: a re-run downloads the same bytes (npm pack is
//! content-addressed per version).

/// One package pair of the corpus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorpusEntry {
    pub package: &'static str,
    pub old_version: &'static str,
    pub new_version: &'static str,
    /// The build's path inside `package/` of both tarballs.
    pub file: &'static str,
}

const fn entry(
    package: &'static str,
    old_version: &'static str,
    new_version: &'static str,
    file: &'static str,
) -> CorpusEntry {
    CorpusEntry {
        package,
        old_version,
        new_version,
        file,
    }
}

/// The corpus, in run order.
pub const CORPUS: &[CorpusEntry] = &[
    entry("underscore", "1.12.1", "1.13.4", "underscore.js"),
    entry("lodash", "4.17.20", "4.17.21", "lodash.js"),
    entry("moment", "2.29.1", "2.29.4", "moment.js"),
    entry("dayjs", "1.10.7", "1.11.10", "dayjs.min.js"),
    entry("axios", "1.6.0", "1.7.9", "dist/axios.js"),
    entry("bluebird", "3.7.0", "3.7.2", "js/release/bluebird.js"),
    entry("q", "1.5.0", "1.5.1", "q.js"),
    entry("ramda", "0.27.2", "0.29.1", "dist/ramda.js"),
    entry("immutable", "4.0.0", "4.3.7", "dist/immutable.js"),
    entry("async", "3.2.0", "3.2.6", "dist/async.js"),
    entry("minimist", "1.2.5", "1.2.8", "index.js"),
    entry("ms", "2.1.2", "2.1.3", "index.js"),
    entry("jquery", "3.6.0", "3.7.1", "dist/jquery.js"),
];
