# The off-box backup

The server holds the only copy of every blob: events replicate to each device, blob bytes do not. The
backup carries everything a rebuild of the server needs, to an object store outside its provider
(Cloudflare R2, ruling 44).

## What it holds

| Prefix | Content | Cadence | Bucket lock |
|---|---|---|---|
| `blobs/` | every blob, named by its hash | hourly | 30 days |
| `events/` | the full event log, xz-compressed (~4 MB at 30k events) | nightly, 30 kept | 7 days |
| `config/` | secrets and config | nightly | 7 days |

Tables are not backed up. Every projection is rebuilt from the event log by replay, so the 1.1 GB on
disk is mostly derived data. Any server state that is not an event is inventoried at build time and
added to `config/` if it cannot be rebuilt.

## Why it is shaped this way

- **Encrypted before it leaves the box.** R2 encrypts at rest with Cloudflare's keys, which protects
  their disks but not against their staff, a legal demand, or anyone holding the account or token.
  Client-side encryption (rclone `crypt`, contents and names) means the bucket only ever holds
  ciphertext. Compression happens before encryption, since ciphertext does not compress.
- **The key lives off the box.** It is generated in the owner's password manager and copied to the
  box's secrets, so a dead box does not take the key with it. Lose the key and the backup is
  unreadable, by design.
- **Uploads only add.** Blobs are immutable and content-addressed, so the job copies new ones and
  never mirrors a deletion: a wiped or corrupted box cannot empty its own backup.
- **Deletions follow purges, not absence.** A blob leaves the backup only when the server holds a
  purge record for it, so retention rulings still reach the backup and a missing file does not.
- **Bucket locks per prefix** block delete and overwrite for N days from each object's upload. A
  stolen token or a bug cannot touch anything recent. The cost is that a purged blob lingers until its
  lock expires.
- **The token is the narrowest level that works:** Object Read & Write on this one bucket. It cannot
  edit bucket configuration, so it cannot lift a lock (inferred from the docs; tested at build).

rclone beat restic: restic's prune rewrites and deletes its own pack files, which fights a lock,
and its snapshots add little when every blob is immutable. It beat custom code by being the
maintained thing that already exists.

## Size

About 410 MB of blobs and ~190 MB of exports, against R2's free 10 GB-month of *stored* data (uploads
count only as operations, and egress is free). Blobs are what grows. The job reports bucket size in
its status and warns before 10 GB.

## Done means a restore

The job counts as working only once a restore has been checked. A restored blob must hash to its own
name, which makes integrity free to verify.
