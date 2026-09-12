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

It does not run a model. Deriving text from a PDF's text layer is deterministic and happens here;
reading a scan is not, and arrives later as `document_text_transcribed`.

## Counting a backfill honestly

A batch run reports `seen`, `archived`, `failed`, and separately `without_text`, with
`seen == archived + failed` asserted rather than assumed.

A bare count of successes is unfalsifiable: a file that falls out of the loop unclassified is
invisible, and over several hundred documents a clean-looking partial import is the failure that
costs the most to find later. `without_text` is broken out because a run that archives seven
hundred files and can read none of them is technically a complete success and practically a
problem. Failures are sampled into the log rather than counted, because a number tells nobody
which files to go and look at.

## Blobs, and what content-addressing does not mean

Files are stored under their own sha256, so storing the same bytes twice is a no-op and a stale
blob cannot exist.

**Identical bytes are one blob and never one document.** A statement that arrives by email and
the same statement scanned from paper are one file on disk and two entries in the archive — they
arrived differently, and each filing is real. That is why an archive entry carries its own
identity rather than keying on the hash, which would silently merge them.
