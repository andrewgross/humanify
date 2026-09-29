/**
 * The pinned corpus (exp092): REAL npm packages, two published versions
 * each, matched against the package's OWN shipped single-file build (the
 * `file` path inside both tarballs — no rebuild, so nothing the harness
 * does can perturb the ground truth; what changed between the versions
 * is whatever the package's own toolchain shipped, which is exactly the
 * pipeline's real input regime).
 *
 * Chosen for diversity (utilities, dates, promises, networking, DOM,
 * data structures), a CJS-or-UMD single-file build present at the SAME
 * path in both versions, and version pairs one realistic release
 * distance apart. Pinning is exact: a re-run downloads the same bytes
 * (npm pack is content-addressed per version).
 */
export interface CorpusEntry {
  package: string;
  oldVersion: string;
  newVersion: string;
  file: string;
}

export const CORPUS: CorpusEntry[] = [
  {
    package: "underscore",
    oldVersion: "1.12.1",
    newVersion: "1.13.4",
    file: "underscore.js"
  },
  {
    package: "lodash",
    oldVersion: "4.17.20",
    newVersion: "4.17.21",
    file: "lodash.js"
  },
  {
    package: "moment",
    oldVersion: "2.29.1",
    newVersion: "2.29.4",
    file: "moment.js"
  },
  {
    package: "dayjs",
    oldVersion: "1.10.7",
    newVersion: "1.11.10",
    file: "dayjs.min.js"
  },
  {
    package: "axios",
    oldVersion: "1.6.0",
    newVersion: "1.7.9",
    file: "dist/axios.js"
  },
  {
    package: "bluebird",
    oldVersion: "3.7.0",
    newVersion: "3.7.2",
    file: "js/release/bluebird.js"
  },
  { package: "q", oldVersion: "1.5.0", newVersion: "1.5.1", file: "q.js" },
  {
    package: "ramda",
    oldVersion: "0.27.2",
    newVersion: "0.29.1",
    file: "dist/ramda.js"
  },
  {
    package: "immutable",
    oldVersion: "4.0.0",
    newVersion: "4.3.7",
    file: "dist/immutable.js"
  },
  {
    package: "async",
    oldVersion: "3.2.0",
    newVersion: "3.2.6",
    file: "dist/async.js"
  },
  {
    package: "minimist",
    oldVersion: "1.2.5",
    newVersion: "1.2.8",
    file: "index.js"
  },
  { package: "ms", oldVersion: "2.1.2", newVersion: "2.1.3", file: "index.js" },
  {
    package: "jquery",
    oldVersion: "3.6.0",
    newVersion: "3.7.1",
    file: "dist/jquery.js"
  }
];
