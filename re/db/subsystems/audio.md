# audio: the game's Wwise integration

Owner: see `subsystems.tsv`.

## Map
- Wwise job workers. The sound engine init 0x143fbf830 gives Wwise `Snd_Request_JobWorker` (0x143fbdd00) as its request-job-worker callback. The callback hands each requested worker to an idle mailbox of the 12 audio job worker threads (mailboxes of 0x30 bytes at 0x146c63bd0: +0x00 thread id, +0x08 fn, +0x10 wake event, +0x24 job type, +0x28 busy). It queues the rest in `g_Snd_JobWorkerOverflow` (24 slots of 0x30 at 0x146c63e10, first free) and counts them in `g_Snd_JobWorkerPending`, all under `g_Snd_JobWorkerLock`.
- `Snd_Run_JobWorkerThread` (0x143fc6500) runs its mailbox request, then loops while the pending count is non-zero over overflow slots 0-11 only. A request queued in slots 12-23 never runs: every worker spins in that loop, the Wwise audio thread (0x144ce2a40) waits forever for its job group while holding the Wwise main lock 0x145fbd440, and the next `AK::SoundEngine::SetRTPCValue` that finds the command queue full blocks a game job. The watchdog (0x1443e0b80) kills the process after 30 s with the crash stub 0x14391f740 (writes 0xdf3b9606 to address 1). Seen in a 1990 race on leaving a replay (session 20261030-113325).
- f1_1990_audiofix.dll (addons/f1-1990) wraps `Snd_Request_JobWorker`. It passes a thunk in place of each worker function; after every request it handles, the thunk moves queued requests from slots 12-23 into free slots 0-11 under the lock. The request wrap does the same after each call.

## Inbox
