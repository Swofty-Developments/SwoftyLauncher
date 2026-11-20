# Sourced by decomp, ghidra_apply, ghidra_export, ghidra_clone.
RE=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)
GH=${GH:-/nix/store/ykjg7k1x4zxc8xv7l8mklj3lggr3mfwj-ghidra-12.1.2}
RE_GHIDRA_POOL=${RE_GHIDRA_POOL:-4}
RE_GHIDRA_XMX=${RE_GHIDRA_XMX:-8G}
# The Ghidra scripts read one folder of tables: kb.py export merges re/db with the binding records in re/bindings.
if [[ -n ${RE_KB:-} ]]; then
    KB=$RE_KB
else
    KB=$RE/ghidra/kb
    python3 "$RE/tools/kb.py" export "$KB" >/dev/null
fi
# kb.py export swaps a symlink to a new folder; pinning the folder keeps one export for the whole job.
KB=$(readlink -f "$KB")
MASTER=$RE/ghidra/master
SCRIPTS=$RE/ghidra/scripts
PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)

kb_stamp() {
    (cd "$KB" && for f in functions.tsv globals.tsv structs/*.h; do if [[ -f $f ]]; then echo "== $f"; cat "$f"; fi; done; cat "$SCRIPTS/KbApply.java") | sha1sum | cut -c1-12
}

ensure_project() {
    local dir=$1
    [[ -f $dir/f1.gpr ]] && return 0
    mkdir -p "$dir"
    exec {cfd}>"$RE/ghidra/.create.lock"
    flock "$cfd"
    if [[ ! -f $dir/f1.gpr ]]; then
        [[ -e $MASTER/f1.lock ]] && { echo "master is locked, cannot clone" >&2; return 1; }
        cp -a "$MASTER/f1.rep" "$dir/" && cp -a "$MASTER/f1.gpr" "$dir/"
    fi
    exec {cfd}>&-
}

lock_project() {
    local dir=$1 mode=${2:-wait}
    ensure_project "$dir" || return 1
    exec {PROJ_FD}>"$dir/re.lock"
    if [[ $mode == try ]]; then
        flock -n "$PROJ_FD" && return 0
        exec {PROJ_FD}>&-
        return 1
    fi
    flock "$PROJ_FD"
}

acquire_slot() {
    if [[ -n ${RE_GHIDRA_PROJECT:-} ]]; then
        SLOT=$(readlink -f "$RE_GHIDRA_PROJECT")
        lock_project "$SLOT" wait
        return
    fi
    local waited=0
    while :; do
        for ((k = 0; k < RE_GHIDRA_POOL; k++)); do
            SLOT=$RE/ghidra/slots/s$k
            lock_project "$SLOT" try && return 0
        done
        ((waited++ % 20 == 0)) && echo "decomp: all $RE_GHIDRA_POOL slots busy, waiting" >&2
        sleep 0.5
    done
}

headless() {
    local dir=$1; shift
    GHIDRA_HEADLESS_MAXMEM=$RE_GHIDRA_XMX "${PIN[@]}" "$GH/lib/ghidra/support/analyzeHeadless" "$dir" f1 \
        -process F1_25.exe -noanalysis -scriptPath "$SCRIPTS" "$@"
}
