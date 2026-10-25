#!/usr/bin/env bash
# exp151: warm neutrality vs main, default then report mode (the /3 index); a dead endpoint so a cache miss fails loud.
set -uo pipefail
cd /work/exp151-edges
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
EP=http://127.0.0.1:9/v1
npm run eval -- neutrality 2a24ae54e 2.1.215:2.1.216 --workdir /work/exp151-neutral-default --priors /work/exp050-cold \
  --cache /work/neutrality-cache --endpoint $EP > /work/exp151-data/neutrality-default.log 2>&1
echo "default rc=$?" >> /work/exp151-data/neutrality.rc
npm run eval -- neutrality 2a24ae54e 2.1.215:2.1.216 --workdir /work/exp151-neutral-report --priors /work/exp050-cold \
  --cache /work/neutrality-cache --endpoint $EP --pipeline-set library.matching=report \
  --pipeline-set library.index=/work/library-index/current > /work/exp151-data/neutrality-report.log 2>&1
echo "report rc=$?" >> /work/exp151-data/neutrality.rc
