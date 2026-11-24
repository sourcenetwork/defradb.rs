import ChildPage

/-! `order_index_scan_params` gives an ordered index scan no offset and a
saturating `offset + limit` budget. The LimitNode above drops the offset.
This assumes the ordered input has at most `max` rows, the representable
range of the scan budget; selecting the right index order is outside scope. -/
namespace OrderedIndexPage

-- Saturating the budget still gives the same page when the stream fits in
-- the representable range. This also covers short streams and zero limits.
theorem saturating_budget_page (rows : List Nat) (offset limit max : Nat)
    (fits : rows.length ≤ max) :
    (rows.take (min max (offset + limit))).drop offset =
      ChildPage.page rows offset limit := by
  by_cases h : offset + limit ≤ max
  · rw [Nat.min_eq_right h]
    exact ChildPage.stop_after_page rows offset limit
  · have enough : rows.length ≤ offset + limit := by omega
    rw [Nat.min_eq_left (by omega), List.take_of_length_le fits]
    rw [← ChildPage.stop_after_page rows offset limit, List.take_of_length_le enough]

end OrderedIndexPage
