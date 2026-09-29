# Privacy and local processing

PhotoFindry performs tagging, face processing and search inference on the Mac.
Original photos remain in their folders. Saved tags, names, previews, embeddings,
queries and processing history are stored locally. No mandatory PhotoFindry
account, advertising analytics or app telemetry is part of the product.

## Explicit internet actions

Model downloads and manual update checks contact their respective hosts. The
beta update feed is served by `photofindry.com`; release downloads are served by
GitHub. Website and Buy Me a Coffee links open a browser when selected. These
actions do not require uploading photographs, names, tags, embeddings or search
queries. Hosts can receive connection metadata, including an IP address, and have
their own privacy practices.

With required models available, photo workflows are designed to work offline.
The current local child-process boundary is not an OS-enforced network/filesystem
sandbox. The bounded observation recorded in [QUALIFICATION.md](QUALIFICATION.md)
does not establish an absolute no-internet guarantee.

The app and engine are proprietary. Source availability, ad-hoc signing or a valid
release signature alone cannot guarantee security. Third-party licences and
notices are included in the app. You can read and export your saved work freely.

## Public problem reports

Reports are voluntary. Review diagnostics for filenames, queries, names and photo
details before sharing. Do not attach photo libraries, model weights, original
photographs or raw private traces to public issues or pull requests.

## Voluntary feedback by email

Send Feedback prepares an email to `photofindry@lyubenov.com`. You review and send
it through your email app. The report contains what you type, optional app/macOS
version and CPU architecture, and only screenshots you choose. The recipient also
receives your sender address. Your email provider handles delivery. No logs,
photo records, names, saved searches or library files are attached automatically.

Screenshot copies remove original file metadata and filenames, but visible
photographs, names and queries remain visible. Redact them before attaching.
Copies may be resized. The form keeps a draft in memory while the app is open;
shared copies use private temporary storage until handoff completes or you choose
Done with Email after sending/closing the message. Abrupt exit can leave copies
in macOS temporary storage. Exported email drafts remain where you save them.
The app does not inspect your mailbox or confirm inbox delivery.
