# Hiding amounts

The eye in the top bar hides every amount on screen with one tap, the way a banking app greys out
balances when you open it in public. The setting stays on until you tap the eye again, including
across restarts. Each device keeps its own setting: hiding amounts on the phone you are about to
hand to someone should not change your desktop.

## One formatter, so nothing slips past

A mask that misses one number is worse than having no mask, because it tells you the screen is
safe when it is not. So every amount the app formats goes through `frontend/src/money.rs`:

- `format_money` for amounts the app pads and groups ("1,234.50 CAD"),
- `amount_as_is` for amounts shown exactly as they arrived ("-12.5 CAD"),
- `figure` for a bare number with no commodity, such as a budget target,
- `input_style` for amount fields, which show dots and still accept typing.

When amounts are hidden, an amount shows as `•••• CAD`. The commodity stays visible and the size
does not. Amounts no longer render as hand-written `"{amount} {commodity}"` strings, and a new
money render should not bring that pattern back.

The setting is stored in `localStorage` and read on the first render. It is not stored in the
continuity store, because that store loads after the first frame, so amounts would flash on
every launch before the mask applied.

## How far it reaches

Settings → Privacy offers three levels, and each includes the one before it. The default is
the widest.

| Level | Hides |
|---|---|
| Amounts | Everything that goes through the formatter: balances, totals, postings, budgets, amount fields. |
| + Text | Money written in free text: assistant replies and proposals, email bodies and subjects, transaction descriptions, document fields, text and CSV previews. |
| + Documents | Covers document and photo previews until you tap Show on each one. |

## What text masking can and cannot catch

Free text has no structure, so `mask_text` looks for patterns. A number is masked when one of
these holds:

- a currency symbol sits next to it (`$412.80`, `€ 12`, `12€`),
- a currency code sits next to it (`412.80 CAD`, `CAD 1,200`),
- a money word follows it (`1800 dollars`),
- it has exactly two decimal places (`-1,450.18`).

In a CSV preview, any cell that holds nothing but a decimal number is also masked, since
statement exports print amounts with no currency beside them.

It leaves version numbers, dates, IP addresses, percentages and plain counts alone. It cannot
catch:

- A bare whole number with nothing marking it as money. "Spent 412 on food" stays visible.
- Amounts inside a PDF or photo. Only the + Documents level covers these, by hiding the whole
  preview.
- The journal and notes editors. These are your own writing, edited in place, and replacing
  their text would corrupt what you type.
- The Ledger query box. The filter you type, such as `amt:>100`, is shown as written.

Chart shapes stay visible when amounts are hidden. They show how a total changed over time, not
how large it is, and the charts carry no axis values.
