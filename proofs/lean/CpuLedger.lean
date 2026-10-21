-- One claim alone: all of its CPUs or none, and a rollback that restores the counters.
import CpuLedger.Hold
-- The books: every counter is the number of Holds that list its CPU, through every operation.
import CpuLedger.Count
-- Holds claimed alone never list one CPU together, through every operation.
import CpuLedger.Exclusive
