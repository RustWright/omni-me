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
