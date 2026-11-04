---- MODULE MC_RetrySchedule_Common ----
EXTENDS RetrySchedule
\* Shared values for the RetrySchedule GREEN/RED configs: Go's doubling ladder,
\* cut to three rungs and measured in ticks of the first rung's half. Three
\* rungs are the fewest on which a peer can fail two passes in a row and still
\* have a longer rung to climb to.
mcLadder == <<1, 2, 4>>
\* A backward step of the wall clock that stays within the longest rung, and
\* one that passes it.
mcSteps == {1, 4}
====
