#!/bin/bash
# Score everything for one weight variant: VARIANT= (plain IDF) or VARIANT=-capvocab
set -e
cd /work/exp095-library-fingerprints/immutable-literals
PY=tools/venv/bin/python
$PY scripts/score.py > logs/score$VARIANT.log 2>&1
$PY scripts/score_stmts.py > logs/score-stmts$VARIANT.log 2>&1
