# Licensing

GraphLite-RS is **dual-licensed**. You may use it under either of the following,
at your choice:

1. **GNU Affero General Public License v3.0** (`AGPL-3.0-only`) — see
   [LICENSE](LICENSE). Free of charge.
2. **A commercial license** — for use cases that AGPL does not permit. Contact
   **luhuizhx@gmail.com**.

This page explains which one applies to you. It is a plain-language summary, not
legal advice; the `LICENSE` file is the governing text for option 1.

---

## What AGPL-3.0 allows, free of charge

AGPL is a strong copyleft licence, but **it does not prohibit commercial use**.
The obligation is triggered by _modification plus network service_, or by
_distribution_ — not by earning money.

| How you use it                                          | AGPL sufficient?                                                                                                       |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Internal use inside your company                        | **Yes**                                                                                                                |
| Personal or academic projects                           | **Yes**                                                                                                                |
| Run it unmodified and expose it over a network          | **Yes** — you must not hide the fact that it is AGPL, but you owe no source to anyone                                  |
| Modify it, then let users interact over a network       | **Yes**, but AGPL §13 requires you to offer those users the **complete corresponding source of your modified version** |
| Bundle it into a product you distribute (closed source) | **No** — this requires a commercial licence                                                                            |
| Sell hosting or support for it                          | **Yes**                                                                                                                |
| Use it to build an open-source product                  | **Yes**, if that product is itself AGPL-compatible                                                                     |

The clause that matters most for a database is §13: if you take the engine,
modify it, and offer it as a hosted service without publishing your
modifications, you are outside the licence.

## When you need a commercial licence

A commercial licence is available if any of the following describes you:

- You want to embed GraphLite-RS in a **closed-source** product that you ship to
  customers.
- You want to offer a **modified** version as a network service without releasing
  your modifications under AGPL.
- Your organisation's policy forbids AGPL-licensed dependencies.
- You want a warranty, indemnity, or a support commitment.

## What a commercial licence provides

Terms are negotiated per engagement. Typically it covers:

- A non-copyleft right to use, modify and distribute the software in closed
  products.
- The right to keep your modifications private.
- Optionally, support and/or a development commitment.

## Contact

**luhuizhx@gmail.com**

Please include: your company, the intended use, whether you will modify the
software, and whether you will distribute it or only operate it as a service.
That is usually enough to give you a straight answer.

---

## Why contributions require a CLA

Dual licensing is only possible while a single party holds sufficient rights to
relicense the whole work. If someone contributes code under AGPL terms alone,
that code can no longer be included in a commercial licence, and the dual-licence
model collapses for the entire project.

For that reason contributions are accepted under the
[Contributor License Agreement](CLA.md), which grants the project the right to
license your contribution under both AGPL and commercial terms. You keep
ownership of your contribution; you are granting a licence, not assigning
copyright.

If you are unwilling to sign, that is a legitimate position — but such a
contribution can only be merged if the project abandons dual licensing, so it
would need to be discussed first.
