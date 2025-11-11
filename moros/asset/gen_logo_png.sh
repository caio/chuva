#!/bin/bash

set -e

INKSCAPE_BIN=${INKSCAPE_BIN:-$(which inkscape)}
test -x $INKSCAPE_BIN

for size in 16 32 192 512; do
    $INKSCAPE_BIN -w $size -h $size --export-type=png --export-filename=logo$size.png logo.svg
done
