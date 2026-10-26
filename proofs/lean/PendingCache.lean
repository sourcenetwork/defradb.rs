import PendingCache.Covers
import PendingCache.Latest
-- The pending log laws: a cache of a load plus every insert logged since covers every snapshot of its generation, a merge that cuts the log anywhere loses no vector, and a search scores each vector by the newest entry at or before its head.
