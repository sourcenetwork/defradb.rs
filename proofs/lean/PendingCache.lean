import PendingCache.Covers
import PendingCache.Latest
import PendingCache.Marks
-- The pending log laws: a cache of a load plus every insert logged since covers every snapshot of its generation, a merge that cuts the log anywhere loses no vector, a search scores each vector by the newest entry at or before its head, and a superseded marker in reach picks that entry without a map.
