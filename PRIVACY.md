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

## Voluntary private feedback

In beta 5, Send Feedback submits your required contact email, report text and
chosen screenshots over HTTPS to https://photofindry.com/bugs. Optional app/macOS
version and CPU architecture are shown before sharing. The website form uses the
same private service. Nothing is sent until you choose Submit Feedback. No logs,
photo records, names, saved queries or library files are attached automatically.

Reports and screenshots are stored for private review through SSH and retained
until the team explicitly removes them. There is no public report lookup. Copies
are re-encoded without original metadata or filenames; visible private content
remains visible. Redact it before submitting. Drafts stay in app memory until exit;
failed or cancelled submissions retain the draft. A cancelled request may already
have arrived; retrying unchanged content reuses its submission identity.

Abuse controls use keyed network identifiers. Counters expire after at most
24 hours; expired rows are removed on the next submission attempt. Infrastructure
can maintain operational connection logs. The form has no analytics. No email is
sent automatically by the intake service. Earlier beta 2 builds used an email
draft and the selected email provider's delivery and retention practices.

## Optional diagnostic attachment

Beta 5 offers an unchecked diagnostic-log option in the app feedback form. It
contains at most 200 recent operational events from the current session, covering
up to 30 minutes. The bounded fields exclude raw messages, queries, names, paths,
photo IDs and image content. You can review the frozen attachment, refresh it or
uncheck it before submission. No log is uploaded until you submit with consent.

## Marketing website

The photofindry.com homepage uses the owner's PostHog EU website analytics with
identified-only person profiles. Documentation and feedback pages do not load
that script. Website analytics are separate from the app, which has no advertising
analytics or app telemetry.
