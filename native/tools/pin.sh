# Sourced by build.sh and the native test scripts. It sets PIN, the prefix of their heavy commands, and SWOFTY_JOBS,
# how many CPUs those commands may run on, which is how many jobs ninja runs at once. Under CI (CI set) or without
# taskset PIN is empty. On a workstation whose process may run on every CPU of SWOFTY_PIN_MASK and more, PIN is
# `taskset -c SWOFTY_PIN_MASK nice -n 10`; when it is already held to fewer (an outer taskset -c 16-31, or a machine
# without those CPUs) that affinity stays and PIN is `nice -n 10`. native/test/pin_test.sh tests swofty_pin_plan.
SWOFTY_PIN_MASK=2-7,10,11,14-31

# swofty_cpus LIST: prints the CPUs of a list such as 0-3,8 one per line.
swofty_cpus() {
    local part cpu
    for part in ${1//,/ }; do
        if [[ $part == *-* ]]; then
            for ((cpu = ${part%-*}; cpu <= ${part#*-}; cpu++)); do echo "$cpu"; done
        else
            echo "$part"
        fi
    done
}

# swofty_pin_plan CI HAS_TASKSET AFFINITY AVAILABLE: sets PIN and SWOFTY_JOBS for a process whose CI variable is CI,
# with taskset when HAS_TASKSET is 1, that may run on the CPU list AFFINITY, AVAILABLE CPUs in all.
swofty_pin_plan() {
    local ci=$1 has_taskset=$2 affinity=$3 available=$4 cpu
    PIN=()
    SWOFTY_JOBS=$available
    if [ -n "$ci" ] || [ "$has_taskset" != 1 ]; then
        return 0
    fi
    PIN=(nice -n 10)
    local -A allowed=()
    for cpu in $(swofty_cpus "$affinity"); do allowed[$cpu]=1; done
    local mask=($(swofty_cpus "$SWOFTY_PIN_MASK"))
    for cpu in "${mask[@]}"; do
        [ -n "${allowed[$cpu]:-}" ] || return 0
    done
    if [ ${#allowed[@]} -gt ${#mask[@]} ]; then
        PIN=(taskset -c "$SWOFTY_PIN_MASK" nice -n 10)
        SWOFTY_JOBS=${#mask[@]}
    fi
}

if command -v taskset >/dev/null; then
    swofty_pin_plan "${CI:-}" 1 "$(taskset -pc $$ | sed 's/.*: //')" "$(env -u OMP_NUM_THREADS -u OMP_THREAD_LIMIT nproc)"
else
    swofty_pin_plan "${CI:-}" 0 "" "$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"
fi
