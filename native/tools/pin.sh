# Sourced by build.sh and the native test scripts. PIN is the prefix of their heavy commands: nice 10 on the 26 CPUs
# 2-7, 10, 11 and 14-31, or nice 10 alone when the process may already run on no more CPUs than that
# (taskset -c 16-31 native/build.sh), so a narrower affinity holds. Without taskset it is empty.
PIN=()
if command -v taskset >/dev/null; then
    PIN=(nice -n 10)
    if [ "$(env -u OMP_NUM_THREADS -u OMP_THREAD_LIMIT nproc)" -gt 26 ]; then
        PIN=(taskset -c 2-7,10,11,14-31 nice -n 10)
    fi
fi
