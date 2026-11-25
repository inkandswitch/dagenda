# Mutation testing: with a diff, only the lines it touches, failing if any
# mutant survives (timeouts hung the tests, so they count as caught); without
# one, everything, or one shard of it (`--shard 0/4`), reporting without
# failing. Results land in target/mutants/mutants.out
out=target/mutants/mutants.out
rm -rf "$out"
# cargo-mutants makes mutants.out, but not the directory it goes in
mkdir -p target/mutants
status=0
if [ -f "${1:-}" ]; then
  cargo mutants --in-diff "$1" --no-shuffle -j 2 --output target/mutants || status=$?
else
  cargo mutants --no-shuffle -j 2 --output target/mutants "$@" || status=$?
fi
# 0 all caught, 2 some missed, 3 some timed out (which counts as caught, and
# hides misses in the exit code); anything else is a usage error or a
# failing baseline
case "$status" in 0 | 2 | 3) ;; *) exit "$status" ;; esac
if [ -s "$out/missed.txt" ]; then
  echo "Survived (each needs a test, or an equivalence note in .cargo/mutants.toml):" >&2
  cat "$out/missed.txt" >&2
  [ -f "${1:-}" ] && exit 1
fi
true
