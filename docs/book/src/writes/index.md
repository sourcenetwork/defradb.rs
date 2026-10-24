# Writes

Every write operation decomposes into engine calls from one vocabulary: content-addressed
puts that cannot conflict, blind merges that commute, ordinary puts and deletes that race
when a decision needs one winner, and appends the engine orders. The next chapter is the
full decomposition.

- [Write scenarios](scenarios.md): what each write operation writes, and which of its
  writes never conflict versus need one winner.
