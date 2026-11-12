#!/bin/sh
# Runs every test of azoL against a compiler: `sh test.sh [path-to-azol]`.
# The compiler is `./azol` when none is given, built first with build.sh
# when it is not there. Other arguments go to tests/run.py, for example
# `sh test.sh ./azol programs format` or `sh test.sh ./azol --jobs 4`.
# A section name or an option as the first argument means `./azol`:
# `sh test.sh errors` runs that section with `./azol`.
# Needs Python 3.
here=$(cd "$(dirname "$0")" && pwd)
azol=$here/azol
case "${1:-}" in
    programs | errors | format | cross | self | --*) ;;
    "") ;;
    *)
        azol=$1
        shift
        ;;
esac
if [ "$azol" = "$here/azol" ] && [ ! -x "$azol" ]; then
    sh "$here/build.sh" || exit 1
fi
exec python3 "$here/tests/run.py" --azol "$azol" "$@"
