#!/bin/bash
# All evaluations for one weight variant (VARIANT= | -capvocab | -plus | -plus-capvocab) and claim rule
# (CLAIM=plain | origin). Needs run-scoring.sh for that VARIANT first. Text output -> logs/*<suffix>.txt
set -e
cd /work/exp095-library-fingerprints/immutable-literals
PY=tools/venv/bin/python
S=$VARIANT; [ "$CLAIM" = origin ] && S=$S-origin
$PY scripts/recognise.py > logs/recognise$S.txt
$PY scripts/breakdown.py 20 0.9 10 > logs/breakdown$S.txt
$PY scripts/lpo.py 20 0.9 10 > logs/lpo$S.txt
$PY scripts/stepb.py 20 0.9 10 8 > logs/stepb$S.txt
$PY scripts/cc_scan.py 20 0.9 10 > logs/cc-scan$S.txt
$PY scripts/cc_msal.py > logs/cc-msal$S.txt
if [ -z "$VARIANT" ] && [ "$CLAIM" != origin ]; then
  $PY scripts/stability.py > logs/stability.txt
  $PY scripts/versions_size.py > logs/versions_size.txt
fi
