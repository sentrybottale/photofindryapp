# PhotoFindry 0.3.0-beta.10 · build 146

This Mac release keeps PhotoFindry's background worker small, so Local AI Search and tagging are much less likely to be paused for memory on 8 GB Macs, and opens your browser reliably after an update. Mac build 146 was published on 21 November 2026. Linux keeps 0.3.0-beta.8 / build 142 and Windows keeps 0.3.0-beta.2 / build 132; both stay on their own signed feeds.

**Less memory while the library is idle.** PhotoFindry's library worker checks your folders and faces in the background. On a large library, macOS kept the memory freed by that work reserved for the worker instead of giving it back, so the worker could grow to about 1.2 GB while doing nothing visible. On an 8 GB Mac that is close to the worker's 1.5 GB safety budget, so starting Local AI Search could pause processing for memory and Resume or Retry would refuse to continue. The worker now returns freed memory to macOS: in the same idle test on a 93,806-photo library it stayed near 80 MB instead of 1.2 GB. The memory safety budgets themselves are unchanged.

**Clearer reason when memory blocks Resume.** If a memory check still refuses Resume or Retry, the message now says what is over budget — for example the library worker, search refinement, PhotoFindry as a whole, or the Mac's own critical memory pressure — and what to do next, instead of a general request to wait.

**Opening the browser after an update.** If your default browser was not already running, finishing an update could report "The local browser opener did not respond" even though the update had completed. PhotoFindry now waits for a browser that is still starting, and still opens only its local address.

Updates preserve the library, originals, model paths, saved lenses and saved work. All inference remains local. The MIT client excludes proprietary engine source and private development history. The beta 16 Mac-app migration channel is unchanged.

See QUALIFICATION.md for the exact candidate evidence and hardware limits. Public delivery is complete only after the delivered-byte, live-feed, installer, recovery and website checks pass.
