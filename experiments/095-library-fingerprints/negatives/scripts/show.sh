#!/usr/bin/env bash
# show.sh <start> <end> [max]: print raw bundle text
B=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
python3 -c "import sys;s=open('$B').read()[$1:$2];m=int(sys.argv[1]);print(s if len(s)<=m else s[:m]+' …')" ${3:-3000}
