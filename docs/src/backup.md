# The off-box backup

The server holds the only copy of every blob: events replicate to each device, blob bytes do not. The
backup carries everything a rebuild of the server needs, to an object store outside its provider
(Cloudflare R2, ruling 44).

## What it holds

| Prefix | Content | Cadence | Bucket lock |
|---|---|---|---|
| `blobs/` | every blob, named by its hash | hourly | 30 days |
| `db/` | a full database export, gzipped (60 MB raw, ~6.5 MB gzipped at 30k events) | nightly, 30 kept | 7 days |
| `config/` | `credentials.toml`, the state directory, and the `OMNI_*` settings | nightly, 30 kept | 7 days |

The export is the same `export` the wipe snapshot takes, so it covers every table, including
server state that is not an event, and a restore is an import rather than a replay. It is refused
when it comes out under half the previous night's size.

The job is `core/src/backup.rs` (feature `backup`, enabled by the server), configured by secrets:
`backup_endpoint` (https only), `backup_access_key_id`, `backup_secret_access_key`, `backup_bucket`,
`backup_key`, and `backup_prefix` (`dev/` on dev, which keeps its objects outside the lock rules).
None set turns the job off; some set is an error naming the rest. Its runs are in `/jobs`. Dev
carries the secrets only while the job itself is being tested, then they come out and `dev/` is
emptied.

Each night it also proves the backup readable: tonight's export is downloaded back and compared
byte for byte, and three random blobs are restored and must hash to their own names.

## Why it is shaped this way

- **Encrypted before it leaves the box.** R2 encrypts at rest with Cloudflare's keys, which protects
  their disks but not against their staff, a legal demand, or anyone holding the account or token.
  Client-side encryption (rclone `crypt`, contents and names) means the bucket only ever holds
  ciphertext. Compression happens before encryption, since ciphertext does not compress. File sizes,
  modification times and the three prefix names stay visible.
- **The key lives off the box.** It is generated in the owner's password manager and copied to the
  box's secrets, so a dead box does not take the key with it. Lose the key and the backup is
  unreadable, by design.
- **Uploads only add.** Blobs are immutable and content-addressed, so the job copies new ones and
  never mirrors a deletion: a wiped or corrupted box cannot empty its own backup.
- **Deletions follow purges, not absence.** A blob leaves the backup only when every document
  holding it was purged and nothing else references it, so retention rulings still reach the backup.
  A blob missing on the server without a purge is a loss: it stays, and the run asks for a restore.
- **Bucket locks per prefix** block delete and overwrite for N days from each object's upload. A
  stolen token or a bug cannot touch anything recent. The cost is that a purged blob lingers until its
  lock expires.
- **The token is the narrowest level that works:** Object Read & Write on this one bucket. It cannot
  edit bucket configuration, so it cannot lift a lock (inferred from Cloudflare's docs, not tested).

rclone beat restic: restic's prune rewrites and deletes its own pack files, which fights a lock,
and its snapshots add little when every blob is immutable. It beat custom code by being the
maintained thing that already exists.

## Size

About 410 MB of blobs and ~200 MB of exports, against R2's free 10 GB-month of *stored* data (uploads
count only as operations, and egress is free). Blobs are what grows. The job reports bucket size in
its status and warns before 10 GB.

## Restoring

Set the same `backup_*` values as rclone remotes (see `Rclone::new` in `backup.rs`), then
`rclone copy blobs: <blob dir>`, take the newest `db:` export and import it, and copy back the
newest `config:` folder. A restored blob must hash to its own name.
