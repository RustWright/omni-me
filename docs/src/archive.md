# The document archive

The archive holds files — a scanned lease, a bank statement, a notice of assessment — and
whatever anyone has since worked out about them. It answers two different questions, and the
difference between them shapes everything below: *find me the document about X*, which the
document's own text answers, and *which documents match this condition*, which it does not.

## Three events, not one

A file entering the archive writes `document_archived`. Anything later read out of it writes
`document_fields_extracted`, as often as needed. A model's reading of a document that carried no
text of its own writes `document_text_transcribed`, also as often as needed. They fold onto one
row.

The split is not bookkeeping. **Ingest knows the bytes and may know nothing else.** A forty-page
scan that no parser understands and no model has read is still a document worth keeping, and an
archive that refuses it is not an archive. Putting the extracted structure in the same event
would mean either refusing that file or writing an empty shell — which is the split again, with
extra steps.

It pays a second time on correction. The log is append-only, so a fix is necessarily another
event; having the shape already is cheaper than discovering it later and naming it worse.

## Where a value came from, and whether anyone checked it

Every field carries two things beside its value: a **source** and a **verified** flag.

Source is one of `parser:<id>`, `model:<name>@<version>`, or `human`. Verified says whether the
value was checked against something real — a statement's closing balance agreeing with figures
the bank states about itself.

**A model's output is never verified**, however confident it sounded. The verification pass this
project already has inspects arithmetic, and there is no arithmetic in "this is a 2023 notice of
assessment". That limit is not a gap to be closed with a confidence score; a number there would
imply a calibration nothing provides. The flag says the honest thing instead, and the archive
page is where a person fixes what it flags.

Verified is not derivable from source, which is why it is stored. A parser can legitimately
produce an unverified value: one of the chequing exports carries no balance column at all, so its
parse is clean and unchecked, and collapsing those two states is exactly how an unverified import
comes to read as a verified one.

## Who produces fields, and in what order

Two producers, tried in the order their answers rank. A statement export is read by
`statement::parse`, which already knows not just what the rows say but *what it was able to
check*. Everything else is described by a model: what kind of document this is, a title a person
would recognise, the date it states, and whatever else is worth finding it by.

The parser goes first, and the model is asked only when no parser recognises the file. Asking a
model about a document a parser can read spends money to produce a value that would lose the fold
anyway.

They also run at different times, and the line between them is the network rather than the kind of
answer they give. The parser needs nothing remote, so it runs at ingest, wherever ingest happens —
a device's background queue, the server, a backfill — and a recognised statement is filed already
searchable by period, row count and closing balance. The model is the half that pays a round-trip
per document, which makes it work for a scheduled batch that can be rate-limited, retried and
paused, not for the request that files a document. So a statement gets its fields immediately and
everything else gets them on the next pass, and neither wait blocks a capture.

Recognising a format is stricter than parsing one, and deliberately so. The import path is told
which format a file is, so its parsers can treat a column they can work without as optional — the
transfer-service map does exactly that with its running balance and its row ids. The archive is
told nothing, so it matches the file's header against a signature first and only then parses.
Without that, two parsers accept the same export and neither can be preferred. A file two
signatures both match gets no parser fields at all rather than a guess, because picking one would
make the winner depend on the order the formats happen to be declared in.

A model's fields are never verified, and there is no confidence score beside them. The finance
extractor has one because arithmetic can check it — postings summing to a stated total. Nothing
here can be checked that way: there is no sum to reconcile in "this is a 2023 notice of
assessment". A number would imply a calibration nothing provides, so the flag says the honest
thing on its own and the archive page is where a person corrects what it marks.

## Why fields fold by origin rather than by arrival

Fields fold **by key, ranked by where they came from**: a human beats a parser, a parser beats a
model, and equal ranks fall back to arrival order.

The obvious rule — latest wins — is wrong here in a way that stays invisible until it costs
someone real work. Extraction gets re-run: a new document kind is added, a prompt improves, a
badly-scanned page is rescanned. Under latest-wins, every one of those passes silently overwrites
the values a person had corrected by hand. Ranking the source means a correction survives any
later machine pass, and a parser checked against the bank's own figures survives a model.

The fallback within a rank matters too. Comparing with "strictly greater" would discard a
*second* human correction of the same field — a save that reports success and changes nothing.

### Removing a field is a correction to nothing

The fold has no removal, so a person removing a field writes it again with an empty value. That
write is human-ranked, so a later re-read cannot bring the value back, which is the point: the
removed fields are mostly ones nobody wants kept, a social insurance number read off a payslip.
The field stays in `fields` to hold its rank; the panel hides it, and a hoisted column (`kind`,
`title`, `document_date`) reads it as never set. `tags` is the exception, where an empty value
already means "every tag taken off" and is kept as such.

## Tags are a field, and that is what makes them removable

A document's tags are one folded field keyed `tags`, whose value is the whole set joined by
commas, hoisted into an array column the filter matches against.

Three other shapes were available and each loses something the fold already provides. Its own
`document_tagged` event would buy a second provenance mechanism beside the first, which is two
places for the human-beats-model rule to be got wrong — the same argument that keeps `kind`,
`title` and `document_date` as keys rather than payload fields. Tags as a column written directly
would skip the rank comparison, so a later model pass would overwrite what a person had typed.

**One field per tag is the shape that looks best and cannot work.** The fold inserts and
overwrites a key; it has no removal, because removal is not expressible in an append-only log
without a tombstone per key. So `tag:groceries` as its own field can be added and can never be
taken off. A single field holding the set makes untagging an ordinary write: the new value is the
set without that tag, and it wins on rank like any other human correction.

The cost is that tags are replace-whole-set rather than add-one, which the write path has to know
— a caller sending one tag would silently clear the rest. Transactions already work this way for
the same reason, so it is the codebase's existing contract rather than a new one.

### Why the column is an array and the value is a string

The field value has to be a string: `documents` is `SCHEMAFULL` and every `DocumentField` column
is spelled out, `value` among them. The queryable column does not, and making it an array is what
keeps `CONTAINS` meaning whole-element equality. Filtering the joined string instead would be a
substring match, so `receipt` would find `receipts-2026` and the two tags would be impossible to
tell apart from a filter.

That is also why the separator is a comma rather than a colon: a colon already splits a
`key:value` tag, and normalization refuses a tag containing the comma, so a set cannot round-trip
into more tags than it went in with.

### Normalizing is the write path's job, and only the write path's

Tags are trimmed and lowercased before they are stored, and a tag with an empty half
(`:x`), an embedded separator, or nothing but whitespace is refused. `FromStr` stays lenient
because it parses tags already in the log, written before any of this existed.

Every comparison downstream is against the stored string, so a tag differing from another only by
case or padding is a second tag. The symptom is not an error: it is a filter returning nothing for
a document that visibly carries the tag.

⚠️ **The one bad tag fails the whole set**, rather than being dropped. A silently skipped tag is a
tag someone typed, watched land, and can never find again.

The frontend does not normalize, and the shared chip editor's sanitizer is explicitly switched off
for the archive: it filters to alphanumerics plus `-_/`, which strips the colon a `key:value` tag
is built on. Two normalizers is how the stored form and the queried form come apart.

## Text travels in the event

A document's text is stored on the archive event rather than derived per device, which looks
backwards until you notice that **blob bytes do not sync**. Devices keep a bounded cache of
files they have opened and re-fetch the rest on demand. A device that never opened a document has
no bytes to read and never will, so text left out of the event would make the archive searchable
only on whichever machine happened to hold the file.

Text has its own provenance for the same reason fields do. `extracted` means the file carried a
text layer and it was lifted out. `transcribed` means a model read it off an image. A born-digital
PDF gives the first; a scan is a photograph of a page and can only ever give the second. Search
over transcribed text is correspondingly less trustworthy, and a reader who cannot tell the two
apart will trust both the same.

`none` is an ordinary outcome. The document is archived and findable by name, and gains text if
something can ever read it.

## How a scan gains text later

`document_archived` is written once, at ingest, and never revised — the same bytes filed twice
are two entries, so there is no second archive event to hide a text update inside. A scan
therefore cannot gain text through the event that created it, and transcribing during ingest
instead would put a network round-trip in the path that files a document: a capture taken offline
would fail to archive, and every document filed before the feature existed would stay unreadable
anyway.

So a transcription is its own event. It is not a reserved key on `document_fields_extracted`
either, for two reasons. Fields fold through a read-modify-write of the whole array, so a
document body would be re-serialized on every later extraction. And field source speaks
`human`/`parser:`/`model:` while text provenance speaks `extracted`/`transcribed`/`none` — one
mechanism made to carry two vocabularies is how a fold rule gets got wrong.

Text folds by origin, exactly as fields do: `extracted` beats `transcribed` beats `none`, with
equal ranks falling back to arrival order so that re-running transcription with a better model
lands rather than silently doing nothing.

**The guard this buys is needed on the archive event, not the transcription.** Events fold in the
order the pull filter delivers them, and that filter runs on the authoring device's clock — so a
phone's transcription can fold *ahead* of the laptop's archive event for the same document. That
archive event carries the absence of text that made transcription necessary. Writing it
unconditionally would overwrite the reading with nothing, report nothing, and leave no way back:
blobs do not sync, so on most devices there are no bytes to re-read.

An unrecognized text source — one a later build introduces — ranks above `none` rather than below
it, so that an older build folding the same log preserves text it cannot account for instead of
erasing it.

## What ingest deliberately does not do

It does not append its own events — it returns them, and the caller persists. A function that
owns an event store cannot be tested without one, and everything interesting about ingest is in
what it makes of a file.

It does not run a model. Lifting a PDF's text layer and parsing a statement's rows are both
deterministic and both happen here; reading a scan is not, and arrives later as
`document_text_transcribed`.

Deriving the parser's fields belongs to ingest itself rather than to each caller of it, and that
placement is the point. Ingest is the one function that knows a document has just been created;
every caller — the HTTP route today, a device capture or a backfill later — gets fields without
opting in, and none of them can forget. Ingest returns two events instead of one, and the caller
appends what it is given.

## Encrypted documents

Around a fifth of a real statement corpus is encrypted — and not evenly: it is institutional
practice, so an issuer that locks its statements locks all of them. The password is never in
the file. Each institution invents its own rule for deriving one, so omni-me holds passwords
rather than rules, in `credentials.toml` under any secret named `pdf_password*`:

```toml
[secrets]
pdf_password_1_globepay  = "..."
pdf_password_2_northwind = "..."
```

**Ingest tries all of them, in name order.** That is the part worth explaining, because trying
one would be tidier: ingest has nothing to select a password *by*. A statement that arrives as
an email attachment carries no issuer identity — only a sender, and the sender list is
deliberately not a gate anywhere in this system. A folder import has a directory name, an
upload has neither. Rather than invent an attribution that only sometimes exists, every
configured password is offered.

The empty password is always tried first, which is what keeps this cheap: an unencrypted
document opens on it, and poppler succeeds on an unencrypted file even when handed a wrong
password, so the majority of a corpus still costs exactly one `pdftotext` run however many
passwords are configured. Only a file that actually reports `Incorrect password` walks the list.

**A document that no password opens is still archived**, textless, with a warning that says how
many were tried. It is never refused and never silently dropped: the bytes are worth keeping,
and the missing text is recoverable later by configuring the password and re-reading. What this
replaces is worse — before 2026-09-27 an encrypted statement was archived as though it simply
had nothing to read, indistinguishable from a blank page.

Both routes into a document's content take the same list, and they have to: the fallback for a
statement whose text cannot be read is to *photograph* it and send the pictures to a model, and
a renderer that cannot decrypt either leaves an encrypted document with no route at all.

**Re-reading happens once per server start.** The password list is read from the credentials
file at boot and never changes while the process runs, so that is the only moment a textless
PDF can become readable. `document_enrichment::reread_textless_pdfs` tries every one of them
then. It costs no model call, so it runs whether or not enrichment is enabled. Text it recovers
is recorded as `extracted` because it is what the file states, not a model's reading. A PDF
that still opens to nothing is left for transcription.

**Viewing goes through the server, so the phone never holds a password.** `GET
/blobs/{hash}/preview` returns an unencrypted copy of an encrypted PDF, re-rendered by
`pdftocairo` and cached beside the image previews. When no password opens a PDF, the server
sends the locked original marked `no-store`, so a device that has already viewed it gets the
readable copy once a password is configured. The cached copy is plaintext on the server's disk.
The server already holds both the password and the extracted text, so the copy exposes nothing
new.

One trade is deliberate and worth stating plainly: **the password is passed to poppler on the
command line**, where anything able to read the process table can see it. There is no
alternative in poppler itself — `pdftotext` and `pdftoppm` both take the value inline, with no
file or stdin variant — so closing it means adding a dependency (`qpdf --password-file=-`) or a
Rust PDF decryption crate. It is recorded at the call site rather than left to be discovered.

## Enriching a document after ingest

The pass ingest defers to is `document_enrichment`, and it runs where the bytes are. Every
document is archived through `POST /documents/archive`, so the server holds every blob and a
server-side pass reaches the whole archive. A document whose bytes are absent is counted and
skipped rather than failed — that is what a document archived somewhere else would look like
from here, and nothing produces one today.

The pass reads the `documents` table, so the server keeps that one projection even though devices
run all the others. Every path that stores a document event on the server folds it there: the
archive route, the enrichment pass's own writes, and `/sync/push`. The last one matters because a
correction made on a phone arrives only by push, and a server blind to it would keep re-reading a
document the user already classified.

**The candidate query is the work queue.** There is no table of pending work, no cursor and no
durable marker. A tick asks for documents that still have no `kind`, reads a few, and appends
what it learns; the fold that lands the answer is the same thing that removes the work. This is
self-healing in the way a queue is not — enable the pass a year after a thousand documents were
filed and the backlog is simply the query's answer, with no migration and no reconciliation for
the case where the two disagreed.

It costs one property, and the trade is deliberate: a document the reader can never answer about
would be selected on every tick forever, and with a per-tick cap it would sit at the head of the
queue and starve everything behind it. So the permanent half of that class is removed at the
query — only MIME types the vision path accepts become candidates, and the rest are counted as
`unreadable_mime` so they cannot be mistaken for documents that do not exist. What remains is the
transient half, a supported file the model happens to fail on, and retrying that is correct.

Retrying it on the very next tick is not. The first run on real documents showed why: a
handwritten note the reader timed out on was the newest uncatalogued document, so with one read
per tick it was selected first every time, and every receipt behind it waited until a retry
happened to succeed. A document that never succeeds would hold them back forever. So a
document a tick skips is **held** out of the query for an hour, doubling on each further skip to
a day. The hold lives in the scheduler's memory, not in the log. A restart retries everything,
which is the self-healing property above surviving intact, and the tick log reports how many are
held so a quiet tick cannot be mistaken for an empty archive.

The pass has two halves and runs both per tick, each capped separately. They compete for nothing:
one selects documents with no `kind` and asks what the document is, the other selects documents
whose `text_source` is `none` and asks what it says. A scan is usually both.

⚠️ **An empty transcription is recorded rather than skipped.** A photograph of a blank page has no
text, and saying so is a real answer — it names the model and the date it looked, and it lifts
`text_source` off `none` so the document stops being a candidate. Skipping instead would re-read
every blank page on every tick forever, which is the same starvation the MIME filter above exists
to prevent. A better model later is simply another event, because equal ranks let the newer value
through.

**Newest first**, because the alternative makes a receipt captured this morning wait behind every
historical document. New arrivals reach the head immediately and the tail still drains, since the
head keeps emptying.

**Off unless enabled, and capped when it is.** Reading the archive costs money per document, so
configuring a role-C model must not be the same act as spending it across everything filed to
date. The cap is the second half: a backlog that drains over ticks shows a badly chosen model
after a handful of documents rather than after the whole corpus.

Every candidate lands in exactly one bucket — read, no bytes, or not read — and the identity is
checked before the tally is returned. Auto-import earned that rule the hard way: a source that
fetched 295 rows and mapped none of them reported `0`, which is bit-identical to being up to
date. A pass that can under-report makes its quiet ticks indistinguishable from its broken ones.

## Reading a document back

Everything renders on the device, from bytes the attachment cache already holds. The archive
exists so a filed document stays readable offline, and a viewer that needed the server would
undo that for the one case it matters in.

That constraint is what decides how each type is drawn. Images go straight into an `<img>`.
A CSV becomes a table of the file's own rows, never the statement parser's reading of them —
correcting a misread value means seeing what the file actually says, so an interpreted table
would hide the very thing being checked. Text and JSON go into a `<pre>`.

**PDFs render through pdf.js, onto a canvas per page.** They used to sit in an `<iframe>`, which
works on desktop webkit and displays *nothing* on Android WebView — it ships no PDF renderer at
all. So the most common type in the corpus failed silently on the device most likely to read it.
Canvases behave identically everywhere. Rasterizing server-side was the alternative and loses on
every axis that matters here: a round-trip per page, no offline viewing, no text selection, and
storage for images derived from files already stored. The rasterizer that does exist belongs to
extraction, where it feeds a vision model, and its page budget is about a model's input size and
has nothing to say about reading.

Only the first thirty pages are drawn, because one canvas per page at device pixel ratio will
exhaust an Android WebView on a long statement. Every page is still counted, and the viewer says
how many it left out: a document that appears to end at page thirty otherwise reads as a
document that does.

A type nothing can draw says so and names the format. It does **not** offer a download link,
which is what the old fallback did for everything it did not understand — and on Android that
link silently does nothing, so the user gets a button that looks like an answer and is not one.
HEIC is in that category today: neither Chrome nor Android WebView decodes it, and the image
pipeline refuses it too, so it was previously classified as viewable and rendered an empty frame.
Transcoding it at ingest remains open; saying so plainly beats a blank.

Classification reads the declared type first and the filename only when that says nothing
specific. Bulk ingest labels files `application/octet-stream` whenever the extension is
unfamiliar, so a viewer keyed on the MIME alone would route most of the corpus's CSVs to "no
preview".

## Counting a backfill honestly

A batch run reports `seen`, `archived`, `failed`, and separately `without_text` and
`with_parser_fields`, with `seen == archived + failed` asserted rather than assumed.

A bare count of successes is unfalsifiable: a file that falls out of the loop unclassified is
invisible, and over several hundred documents a clean-looking partial import is the failure that
costs the most to find later. `without_text` is broken out because a run that archives seven
hundred files and can read none of them is technically a complete success and practically a
problem. `with_parser_fields` is the same measure from the other side: format discovery matches a
signature against real exports, so a run over several hundred statements that recognises none of
them has quietly degraded to filename search while every other number still reads as perfect.
Failures are sampled into the log rather than counted, because a number tells nobody which files
to go and look at.

`archived` counts documents, not events, and the two are no longer the same number: a file a parser
recognises yields two. Deriving the count from the event list instead would make the check written
to catch a miscount produce one, and produce it only on the runs that went well.

## Purging, and why a deletion has to be an append

A purge is the one irreversible thing the archive does, and the only place this
project deletes a person's data. It writes `document_purged`, tombstones the row, and
reclaims the blob when nothing else references it.

**It cannot be a deletion, because state is a fold over an append-only log.** Removing the
`document_archived` event is not possible — there is no per-event delete, and sync would
re-append it from the server anyway — and removing the projection row achieves nothing,
because a rebuild replays the log and recreates it. A row recreated that way is the
dangerous case: it looks like an ordinary document and points at bytes that are gone. So
the purge is another event, and the fold is what makes the row come back *purged*.

The tombstone folds with an `UPSERT` rather than an `UPDATE`, which is the lesson routines
paid for. A purge can arrive **before** the archive event it purges, because the pull filter
runs on the authoring device's clock. A bare `UPDATE` matches nothing, and nothing ever
retries a no-op'd mutation, so the archive event would then arrive and materialize a fully
visible row for a document whose bytes had already been deleted.

The same reasoning is why the text guard lives where both writers of text pass through it.
An archive event carries `extracted` and a transcription carries `transcribed`; both
outrank the `none` a purge leaves behind, so either would restore a purged document's words
— on a device that never held the bytes and has no way to notice.

### What a purge does and does not reclaim

It reclaims the **blob**. It does not empty the log: the archive event still carries the
document's text, and always will.

What it removes from every *read* surface is the projection row's content. That is what
takes a purged spam email out of search, out of `list` and `read`, and out of the embedding
index. The words survive in a place nothing reads them back from, and pretending otherwise
would be the more comfortable description rather than the true one.

Hiding it from the vector index is one line — the catalogue entry declares `purged` as its
hidden column — and it is not optional. The sweep selects hidden rows rather than filtering
them out, precisely so that a row indexed while it was visible has its chunks deleted;
without that, a purged document stays reachable by vector search after every other path has
stopped returning it.

That one line also exposed a door that had been closed by a prohibition rather than a
mechanism. Child collections are fetched by a query that knew nothing about hidden rows, so
the rule was "no child may read a table that hides rows" — and `documents` is
self-referential, because an attachment is a document. Teaching the fetch to apply the
hidden rule of whatever table it reads opened the shape legitimately, and found that
removed routine items had been reaching the assistant the same way, while the rollup
computed over the same items filtered them.

### Counting what still needs the bytes, from two different places

Blobs are shared: the same statement emailed and scanned is one file and two entries. So
bytes are reclaimable only when nothing else references them, and "nothing else" spans two
kinds of referrer — other documents, and transaction attachments.

The two are read from different stores, and that is not an inconsistency. Documents come
from the projection. Transaction attachments come from the **event log**, because the
server is the only host holding blobs and therefore the only one that can delete them — and
the server maintains the documents projection alone. `transactions` does not exist there,
and querying a missing table is a hard error rather than an empty set. A count that assumed
otherwise would fail loudly on the server, or, worse, quietly report no references and
delete bytes a committed receipt still points at.

The count is deliberately conservative: the log has no cheap notion of current visibility,
so a transaction the user later deleted still holds its bytes. Over-counting keeps bytes
that might be reclaimable; under-counting destroys bytes something needs. Only one of those
is recoverable.

### Preview, then confirm, and the confirm can only shrink

Deletion is irreversible and therefore never grantable to autonomy: the user confirms, and
the model may only flag. Per-item confirmation is what that rule originally implied, and it
does not survive contact with a mail backfill — one newsletter sender is hundreds of taps,
and the storage the purge exists to reclaim stays occupied until they finish.

So confirmation is per **group**, once, with every document in it listed and individually
sparable. That satisfies what the rule is for — nothing irreversible happens without the
person having seen what goes — while staying usable at the scale the archive actually
reaches. Approving a *rule* instead, and letting matches purge unattended, was refused: it
buys the same scale by spending the thing the rule protects.

The preview mints a ticket, and the confirm may only act on a subset of the ids that
preview returned. Sparing rows shrinks the set, which is the point of the checkboxes;
nothing can widen it. That is what stops a caller purging a group nobody looked at. If the
group is larger than one preview page, the confirm covers the listed rows only and the
screen says so, because a confirm reaching rows the user never saw is exactly what the
listing exists to prevent.

Both numbers are reported rather than netted off: bytes that will actually come back, and
bytes that will not because something else still holds them. "Frees 31 MB" and "frees
0.3 MB of the 31 MB you selected" are different answers, and on a corpus that shares bytes
the second is the true one.

The result asserts `selected == purged + skipped + failed` rather than assuming it, the
same discipline a backfill report follows and for a sharper reason: a bare count of
successes is unfalsifiable, and for an irreversible operation a gap between "selected" and
"done" is the worst thing to discover later. Already-purged is *skipped*, not failed — two
devices can confirm the same group.

## Blobs, and what content-addressing does not mean

Files are stored under their own sha256, so storing the same bytes twice is a no-op and a stale
blob cannot exist.

**Identical bytes are one blob and never one document.** A statement that arrives by email and
the same statement scanned from paper are one file on disk and two entries in the archive — they
arrived differently, and each filing is real. That is why an archive entry carries its own
identity rather than keying on the hash, which would silently merge them.

## What the assistant can see

The archive is one catalogue entry, which is the whole of its assistant integration: the
catalogue drives keyword search, the embedding sweep, `list` and `read` alike, so registering
`document` brings all four at once and forgetting to register it would have removed all four
just as quietly.

### It is the first corpus the user did not write

Every catalogued type before it is authored or accepted by the user: journal entries and notes
are written by them, routines arranged by them, beliefs proposed and then accepted by them. A
document is neither. An archived email was written by whoever sent it, and its attachments by
whoever made them — the archive takes every fetched message unconditionally, which is what
makes it useful and is also the whole of the change in trust level.

The consequences are worth stating rather than discovering. The retrieval corpus now contains
text an outsider chose; a passage retrieved from an email is cited to the user the same way a
passage from their own journal is, and only the record type distinguishes them. The system
prompt already says that record text is data and never instructions, which was written when
every record was the user's own words and matters considerably more now.

Two structural properties carry the weight, and neither is new: the assistant changes nothing
without a proposal the user accepts, and the read verbs reach no network. Both were designed
against exactly this, and both predate the archive.

**This is also the point where the planned dual-model split stops being a nicety.** The
assistant documentation describes a privileged model that holds the verbs and never sees a raw
email body, with extraction quarantined behind it, and notes that the separation "partly exists
already" only as a coincidence of how things were built. That coincidence ends here: `read` on
an archived `.eml` returns its headers and body straight to the model holding the tools. The
split is now load-bearing rather than tidy, and the archive is the reason.

Three of its declarations are not the obvious choice, and each is a consequence of the same
fact — **every archive column is optional**, because fields extracted on one device can be
folded before the event that created the row.

**The handle is the filename, not the title.** A title only exists once fields have been
extracted, which for most of the corpus has not happened and may never; a filename is written at
ingest for everything. A handle that is usually absent degrades to a bare ULID in front of the
model, which is worse than a plain filename in every case and better in none.

**The filename is also a search field.** A scan carries no extracted text until a model
transcribes it, so for those documents the filename is the only thing any index can match.
Leaving it out would make a large and growing part of the archive reachable only by already
knowing its identifier.

**Listing is ordered by when a document was archived, not by the date printed on it.** The
printed date comes from field extraction and is absent wherever that has not run, which would
sort most of the corpus into one undifferentiated block at whichever end nulls land.

An email's attachments are returned as a child collection of the email, which makes `documents`
the only self-referential entry in the catalogue — an attachment is a document, independently
searchable and readable, and the parent link is a link rather than ownership. Without the
collection the link is recorded and unreachable: reading the email would say nothing about the
statement that arrived inside it.

Two columns are withheld. `sha256` is the blob's content address and `device_id` names the
machine that ingested it; both are plumbing, and a model handed a 64-character hex string inside
a record described as a document will quote it back as though it identified something a person
would recognise.

### The trap in optional text columns

The embedding sweep concatenates a record's text fields in SurrealQL. `string::concat` renders an
absent column as the literal string `NONE` rather than erroring or propagating emptiness, so an
untitled scan with no readable text embeds and indexes the word "NONE" as though the document
said it — and a chunk retrieved on that basis is handed to the model as the document's content.
Nothing about the run looks wrong: the sweep succeeds and reports a healthy count.

Every catalogued type before this one declares its text columns `TYPE string`, so the sweep had
never met an absent one. The fix is a coalesce in the concatenation, and the test asserts the
uncoalesced expression's return value directly — a test that only checked the indexed text would
pass whether or not the hazard still existed, and would therefore stop meaning anything the day
someone simplified the coalesce away.
