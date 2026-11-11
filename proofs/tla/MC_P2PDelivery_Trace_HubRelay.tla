------------------------ MODULE MC_P2PDelivery_Trace_HubRelay ------------------------
\* Three nodes: a replicates to hub h, and h to b. a creates a document
\* (h1) and, once b has it, updates it (h2). h merges each head, and its
\* merge leaves the forward intent that becomes h's marker for b.
\* Written by conformance::model_trace from the nodes' model_trace
\* events. Regenerate it rather than edit it.
EXTENDS Trace_P2PDelivery

\* The nodes, by their role in the scenario.
tNodes == {"a", "h", "b"}
\* The replicator edges the scenario set.
tRepl == {<<"a", "h">>, <<"h", "b">>}
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
  [ev |-> "Crash", node |-> "h", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 3: h starts
  [ev |-> "RestoreReplicators", node |-> "h", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 4: h restores its replicators
  [ev |-> "Crash", node |-> "b", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 5: b starts
  [ev |-> "RestoreReplicators", node |-> "b", peer |-> "none", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 6: b restores its replicators
  [ev |-> "Reconnect", node |-> "a", peer |-> "h", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "full"], \* 7: a connects to h (full)
  [ev |-> "Reconnect", node |-> "h", peer |-> "b", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "full"], \* 8: h connects to b (full)
  [ev |-> "Reconnect", node |-> "a", peer |-> "b", head |-> "none", outcome |-> "none", sup |-> {}, mode |-> "full"], \* 9: a connects to b (full)
  [ev |-> "Commit", node |-> "a", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 10: a commits h1
  [ev |-> "Register", node |-> "a", peer |-> "h", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 11: a registers h1 for h
  [ev |-> "Admit", node |-> "h", peer |-> "a", head |-> "h1", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 12: h admits h1 from a: registered
  [ev |-> "Schedule", node |-> "h", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 13: h dispatches h1 (run)
  [ev |-> "Ack", node |-> "a", peer |-> "h", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 14: a has h's ack for h1
  [ev |-> "Acquire", node |-> "h", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 15: h takes permits for h1
  [ev |-> "Merge", node |-> "h", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 16: h merges h1
  [ev |-> "Register", node |-> "h", peer |-> "b", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 17: h registers h1 for b
  [ev |-> "Admit", node |-> "b", peer |-> "h", head |-> "h1", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 18: b admits h1 from h: registered
  [ev |-> "Schedule", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 19: b dispatches h1 (run)
  [ev |-> "Ack", node |-> "h", peer |-> "b", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 20: h has b's ack for h1
  [ev |-> "Acquire", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 21: b takes permits for h1
  [ev |-> "Merge", node |-> "b", peer |-> "none", head |-> "h1", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 22: b merges h1
  [ev |-> "Commit", node |-> "a", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 23: a commits h2
  [ev |-> "Register", node |-> "a", peer |-> "h", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 24: a registers h2 for h
  [ev |-> "Admit", node |-> "h", peer |-> "a", head |-> "h2", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 25: h admits h2 from a: registered
  [ev |-> "Schedule", node |-> "h", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 26: h dispatches h2 (run)
  [ev |-> "Ack", node |-> "a", peer |-> "h", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 27: a has h's ack for h2
  [ev |-> "Acquire", node |-> "h", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 28: h takes permits for h2
  [ev |-> "Merge", node |-> "h", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 29: h merges h2
  [ev |-> "Register", node |-> "h", peer |-> "b", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 30: h registers h2 for b
  [ev |-> "Admit", node |-> "b", peer |-> "h", head |-> "h2", outcome |-> "registered", sup |-> {}, mode |-> "none"], \* 31: b admits h2 from h: registered
  [ev |-> "Schedule", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "run"], \* 32: b dispatches h2 (run)
  [ev |-> "Ack", node |-> "h", peer |-> "b", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 33: h has b's ack for h2
  [ev |-> "Acquire", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"], \* 34: b takes permits for h2
  [ev |-> "Merge", node |-> "b", peer |-> "none", head |-> "h2", outcome |-> "none", sup |-> {}, mode |-> "none"] \* 35: b merges h2
>>
=============================================================================
