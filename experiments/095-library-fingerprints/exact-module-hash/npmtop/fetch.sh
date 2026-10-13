#!/bin/bash
# Fetch abbreviated npm registry metadata for the sample of top packages (names in sample.txt) -> meta/<name>.json
mkdir -p meta
cat sample.txt | xargs -P 32 -I{} sh -c 'f="meta/$(echo {} | sed "s#/#__#").json"; [ -s "$f" ] || curl -s -H "Accept: application/vnd.npm.install-v1+json" "https://registry.npmjs.org/$(echo {} | sed "s#/#%2f#")" -o "$f"'
