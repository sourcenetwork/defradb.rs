------------------------ MODULE MC_P2PDelivery_Trace_TwoNode ------------------------
\* Two nodes: a replicates to b. a creates a document (h1) and, once b
\* has it, updates it (h2). Each head is committed, registered, pushed,
\* admitted as a durable record, dispatched and merged on b.
\* Written by conformance::model_trace from the nodes' model_trace
\* events. Regenerate it rather than edit it.
EXTENDS Trace_P2PDelivery

\* The nodes, by their role in the scenario.
tNodes == {"a", "b"}
\* The replicator edges the scenario set.
tRepl == {<<"a", "b">>}
\* The document's heads, in commit order:
\*   h1 = bafyreihbxwvuibeaueino4obs67hhlnbzw2d7z2tv4itwp37jhvtadc3bq
\*   h2 = bafyreifepj542teubzabuyx7bwomvwfjpu5wzjavuviy24rg4vpoj3rvaq
tHeads == {"h1", "h2"}
\* The node that committed each head.
tAuthor == [hd \in tHeads |-> CASE hd = "h1" -> "a" [] hd = "h2" -> "a"]
\* The heads each head was written on top of.
tParent == [hd \in tHeads |-> CASE hd = "h1" -> {} [] hd = "h2" -> {"h1"}]
\* Each head's priority, one more than its highest parent's.
tPrio == [hd \in tHeads |-> CASE hd = "h1" -> 1 [] hd = "h2" -> 2]
\* The heads in CID byte order.
tKeyOrder == <<"h2", "h1">>
\* No node starts with a durable record.
tBacklogs == {[n \in tNodes |-> {}]}
\* No shared base, trickling provider or dead node: every head is written in the run.
tNoHeads == {}
\* The provider of the advisory pull fetches, if any ran.
tPullPeer == "none"
\* The recorded events, in time order.
tTrace == <<
  [ev |-> "Crash", node |-> "a", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 1: a starts
  [ev |-> "RestoreReplicators", node |-> "a", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 2: a restores its replicators
  [ev |-> "Crash", node |-> "b", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 3: b starts
  [ev |-> "RestoreReplicators", node |-> "b", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 4: b restores its replicators
  [ev |-> "Reconnect", node |-> "a", peer |-> "b", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "full"], \* 5: a connects to b (full)
  [ev |-> "Commit", node |-> "a", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 6: a commits h1
  [ev |-> "Register", node |-> "a", peer |-> "b", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 7: a registers h1 for b
  [ev |-> "Admit", node |-> "b", peer |-> "a", head |-> "h1", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 8: b admits h1 from a: registered
  [ev |-> "Schedule", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 9: b dispatches h1 (run)
  [ev |-> "Ack", node |-> "a", peer |-> "b", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 10: a has b's ack for h1
  [ev |-> "Acquire", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 11: b takes permits for h1
  [ev |-> "Merge", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 12: b merges h1
  [ev |-> "Commit", node |-> "a", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 13: a commits h2
  [ev |-> "Register", node |-> "a", peer |-> "b", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 14: a registers h2 for b
  [ev |-> "Admit", node |-> "b", peer |-> "a", head |-> "h2", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 15: b admits h2 from a: registered
  [ev |-> "Schedule", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 16: b dispatches h2 (run)
  [ev |-> "Acquire", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 17: b takes permits for h2
  [ev |-> "Ack", node |-> "a", peer |-> "b", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 18: a has b's ack for h2
  [ev |-> "Merge", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"] \* 19: b merges h2
>>
=============================================================================
