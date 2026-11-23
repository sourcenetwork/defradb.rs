------------------------ MODULE ParentRegistration ------------------------
EXTENDS FiniteSets
CONSTANT Bug
VARIABLES grants, registered, links, unauthorized
vars == <<grants, registered, links, unauthorized>>
Actors == {"author", "other"}
Parents == {"a", "b"}
Init == /\ grants \in SUBSET (Actors \X Parents)
        /\ registered = {} /\ links = {} /\ unauthorized = FALSE

ChangeGrants == /\ grants' \in SUBSET (Actors \X Parents)
                /\ UNCHANGED <<registered, links, unauthorized>>

Register(actor, parent, source) ==
    /\ actor \notin registered
    /\ \/ <<actor,parent>> \in grants
       \/ Bug = source
    /\ registered' = registered \cup {actor}
    /\ links' = links \cup {<<actor,parent>>}
    /\ unauthorized' = (unauthorized \/ <<actor,parent>> \notin grants)
    /\ UNCHANGED grants

AddLink(actor, parent) ==
    /\ actor \in registered
    /\ <<actor,parent>> \notin links
    /\ (<<actor,parent>> \in grants \/ Bug = "link")
    /\ links' = links \cup {<<actor,parent>>}
    /\ unauthorized' = (unauthorized \/ <<actor,parent>> \notin grants)
    /\ UNCHANGED <<grants, registered>>

Next == \/ ChangeGrants
        \/ \E actor \in Actors, parent \in Parents:
            \/ Register(actor, parent, "local")
            \/ Register(actor, parent, "replica")
            \/ AddLink(actor, parent)
Spec == Init /\ [][Next]_vars
AuthorizedAtWrite == ~unauthorized
RegistrationHasParent == \A actor \in registered: \E p \in Parents: <<actor,p>> \in links
=============================================================================
