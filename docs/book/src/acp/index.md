# Access Control

defradb.rs has two authorization scopes, both built on one engine:

- **DAC** (document access control): per-document policy, registered by the document's
  owner at create time, evaluated on every read and write of that document.
- **NAC** (node access control): one policy over the node itself, gating administrative
  and management operations.

Local ACP evaluates inside the request's transaction, on the same snapshot as the
data; Vera decisions happen at the edge, with their staleness documented. The
chapters:

- [The Zanzibar engine](zanzibar.md): policies, relations, tuple-to-userset evaluation,
  the goal-tree checker.
- [DAC and NAC](dac-nac.md): the document and node scopes, registration, the bypass
  privilege.
- [ACP inside the transaction](transactional.md): how decisions are validated, why no
  public window exists, and revocation.
- [Vera](vera.md): external, chain-verified policy decisions.
