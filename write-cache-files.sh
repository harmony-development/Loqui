#!/usr/bin/env bash

trunk_dir="$TRUNK_STAGING_DIR"
file_names="$(find $trunk_dir -iname '*.wasm' -o -iname '*.js' -o -iname '*.webp' -o -iname '*.html' | sed 's/.*\/dist\/\.stage//g')"
out="$trunk_dir/cache_files.js"

echo "const cacheFiles = [" > $out

while IFS= read -r file; do
    if [[ ! "$file" == "/cache_files.js" ]]; then
        echo "    \"$file\"," >> $out
    fi
done <<< "$file_names"

echo "];" >> $out

cat $out "$trunk_dir/sw.js" > "$trunk_dir/_sw.js"
mv "$trunk_dir/_sw.js" "$trunk_dir/sw.js"
rm $out