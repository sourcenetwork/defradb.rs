/-!
# HttpClient.Redirect: a credential stays with the origin it was meant for

The story, in plain words. The node's HTTP client sends a request and follows
the redirects the servers answer with, up to ten of them. A request can carry
credentials: an Authorization header, a Cookie, a Proxy-Authorization header.
The caller sets some of them on the request; the client adds its default
headers to it, once, before the first exchange, unless the request sets the
same header itself. A credential the caller gave is meant for the request's
own origin: its scheme, host and port. So when a redirect points to another
origin, the client takes every credential off the request before it follows
it, and the credentials never come back for the rest of the chain, not even
when a later redirect points home again. A redirect within the origin keeps
them.

A plain-HTTP request that goes through a proxy carries one more credential:
the proxy's own, which the client adds to that one exchange from the system's
proxy settings when the request has no Proxy-Authorization of its own. It is
meant for that hop's proxy, never carried to the next hop.

What must never happen:
* a credential the caller gave (on the request, or as a default) is sent on a
  hop whose origin is not the request's (`credentials_stay_with_their_origin`);
* a proxy credential the client added is sent on any hop but its own (the same
  theorem);
* more than eleven exchanges are made for one request
  (`at_most_limit_plus_one_exchanges`), or a chain of redirects that never ends
  does not fail (`endless_redirects_fail`);
* a redirect within the origin loses a credential
  (`same_origin_redirect_keeps_credentials`), or one to another origin keeps one
  (`cross_origin_redirect_strips_credentials`).

The witnesses at the end show each guard is needed: with the default headers
added again on every hop (the reviewed bug), with origins compared by host
alone, and with no redirect limit, a credential reaches another origin or the
chain runs past the limit.

Rust anchors (crates/net/src/http/client.rs): `Client::execute` (the defaults
merged once), `Client::follow` (the loop of at most `REDIRECT_LIMIT + 1`
exchanges), `Client::exchange` (the proxy credential added to the copy it
sends), `redirect` (the method rules and the origin check, `url.origin() !=
parts.url.origin()`). The url crate's `Url::origin` gives the special schemes
(http, https, ws, wss, ftp) the tuple of scheme, host and port, the port
written or the scheme's default, and every other scheme an opaque origin equal
to no other, so a redirect from such a URL always strips. `HeaderMap::insert`
replaces every value of a name and `remove` takes them all away, as `insert` and
`remove` do here.
-/

namespace HttpClient

-- A URL's scheme: one of the url crate's special schemes, or any other.
inductive Scheme where
  | http | https | ws | wss | ftp
  | other (code : Nat)
  deriving DecidableEq, Repr

-- Whether the url crate gives the scheme an origin of scheme, host and port.
def Scheme.special : Scheme → Bool
  -- Any other scheme has an opaque origin;
  | .other _ => false
  -- the special ones have a tuple origin.
  | _ => true

-- A URL, as far as origins go: its scheme, its host, its port (the one
-- written, or the scheme's default), and the rest (path and query).
structure Url where
  scheme : Scheme
  host : Nat
  port : Nat
  rest : Nat
  deriving DecidableEq, Repr

-- `a.origin() == b.origin()`: both special, and the same scheme, host and port.
-- Two opaque origins are never equal, not even a URL's with itself.
def SameOrigin (a b : Url) : Prop :=
  a.scheme.special = true ∧ b.scheme.special = true ∧
    a.scheme = b.scheme ∧ a.host = b.host ∧ a.port = b.port

-- The check is a plain comparison, so a program can make it.
instance (a b : Url) : Decidable (SameOrigin a b) := by
  unfold SameOrigin; infer_instance

-- Promise: if a has b's origin, b has a's.
theorem SameOrigin.symm {a b : Url} (h : SameOrigin a b) : SameOrigin b a := by
  -- Take the five equalities apart and put them back the other way round.
  obtain ⟨ha, hb, hs, hh, hp⟩ := h
  exact ⟨hb, ha, hs.symm, hh.symm, hp.symm⟩

-- Promise: two URLs that share an origin with a third share it with each other.
theorem SameOrigin.trans {a b c : Url} (h1 : SameOrigin a b) (h2 : SameOrigin b c) :
    SameOrigin a c := by
  -- Chain the equalities through b.
  obtain ⟨ha, _, hs1, hh1, hp1⟩ := h1
  obtain ⟨_, hc, hs2, hh2, hp2⟩ := h2
  exact ⟨ha, hc, hs1.trans hs2, hh1.trans hh2, hp1.trans hp2⟩

-- The header names this model tells apart; every other name is `other`.
inductive Name where
  | authorization | cookie | proxyAuthorization | wwwAuthenticate
  | contentType | contentLength | transferEncoding
  | other (code : Nat)
  deriving DecidableEq, Repr

-- A credential: a header that proves who the sender is.
def Credential (n : Name) : Prop :=
  n = .authorization ∨ n = .cookie ∨ n = .proxyAuthorization

-- Whether a name is a credential can be checked.
instance (n : Name) : Decidable (Credential n) := by
  unfold Credential; infer_instance

-- The headers `redirect` takes off before a hop to another origin.
def stripped : List Name := [.authorization, .cookie, .proxyAuthorization, .wwwAuthenticate]

-- The headers `redirect` takes off when it turns the request into a GET.
def contentHeaders : List Name := [.contentType, .contentLength, .transferEncoding]

-- Promise: every credential is among the headers taken off for another origin.
theorem credential_stripped {n : Name} (h : Credential n) : n ∈ stripped := by
  -- Each of the three credentials is on the list.
  rcases h with h | h | h <;> subst h <;> decide

-- Promise: no credential is a content header, so turning a request into a GET
-- keeps every credential.
theorem credential_not_content {n : Name} (h : Credential n) : n ∉ contentHeaders := by
  -- Each of the three credentials is off the list.
  rcases h with h | h | h <;> subst h <;> decide

-- Who put a header value on the request. This is the model's bookkeeping; the
-- Rust keeps no such tag.
inductive Source where
  -- The caller, on the request itself;
  | request
  -- the client's default headers;
  | default
  -- the client, for the proxy of the hop to this URL.
  | proxy (hop : Url)
  deriving DecidableEq, Repr

-- A header value: who set it, and its bytes.
structure Value where
  source : Source
  data : Nat
  deriving DecidableEq, Repr

-- A header map: a list of names and values.
abbrev Headers := List (Name × Value)

-- `HeaderMap::contains_key`: some value is set for the name.
def has (h : Headers) (n : Name) : Bool := h.any (fun e => decide (e.1 = n))

-- `HeaderMap::remove`: every value of the name taken away.
def remove (h : Headers) (n : Name) : Headers := h.filter (fun e => decide (e.1 ≠ n))

-- `HeaderMap::insert`: the name's values replaced by this one.
def insert (h : Headers) (n : Name) (v : Value) : Headers := (n, v) :: remove h n

-- Every name of a list removed, one after another.
def removeAll (h : Headers) (ns : List Name) : Headers := ns.foldl remove h

-- `Client::execute`: each default header the request does not set itself is
-- added, once, before the first exchange.
def withDefaults (defaults request : Headers) : Headers :=
  defaults.foldl (fun acc e => if has acc e.1 then acc else insert acc e.1 e.2) request

-- A request's method, as far as the redirect rules go.
inductive Method where
  | get | post | head
  | other (code : Nat)
  deriving DecidableEq, Repr

-- The request `follow` carries from hop to hop (`Parts`).
structure Parts where
  method : Method
  url : Url
  headers : Headers
  body : Bool

-- `Client::exchange`'s headers: the request's own, plus, for a plain-HTTP URL
-- with no Proxy-Authorization of its own, the credential of the proxy the
-- system picks for this URL (`proxyAuth`, `none` when it goes direct or the
-- proxy has none), tagged as meant for this hop. The request is not changed.
def sent (proxyAuth : Url → Option Nat) (p : Parts) : Headers :=
  if p.url.scheme = .http ∧ has p.headers .proxyAuthorization = false then
    match proxyAuth p.url with
    -- The proxy has a credential: send it on this hop;
    | some d => insert p.headers .proxyAuthorization ⟨.proxy p.url, d⟩
    -- or there is nothing to add.
    | none => p.headers
  else p.headers

-- The redirect statuses `redirect` follows: 301, 302, 303, 307 and 308.
inductive Code where
  | moved | found | seeOther | temporary | permanent
  deriving DecidableEq, Repr

-- Whether a redirect turns the request into a GET with no body: 301 and 302
-- turn a POST into one, 303 anything but a HEAD, 307 and 308 nothing.
def toGet (m : Method) : Code → Bool
  | .moved | .found => decide (m = .post)
  | .seeOther => decide (m ≠ .head)
  | .temporary | .permanent => false

-- `redirect`: the request for the hop to `to`. The method rules first; then,
-- for another origin than this hop's, every credential taken off.
def redirect (p : Parts) (code : Code) (to : Url) : Parts :=
  -- The method and body the status asks for;
  let q : Parts :=
    if toGet p.method code then
      { p with method := .get, body := false, headers := removeAll p.headers contentHeaders }
    else p
  -- no credentials to another origin;
  let r : Parts := if SameOrigin to p.url then q else { q with headers := removeAll q.headers stripped }
  -- and the new URL.
  { r with url := to }

-- What a server answers one exchange with.
inductive Answer where
  -- Not a redirect the client follows (not 3xx, or 304, or no Location, or
  -- another 3xx): this is the response.
  | final
  -- A Location that is not text or does not join onto the URL.
  | badLocation
  -- A redirect with a status and the URL it joins to.
  | redirect (code : Code) (to : Url)
  -- The exchange itself failed: connect, TLS, protocol.
  | failed
  deriving DecidableEq, Repr

-- How a request ends.
inductive Outcome where
  | response (url : Url)
  | tooManyRedirects
  | error
  deriving DecidableEq, Repr

-- `Client::follow`, with `budget` exchanges left: one exchange per answer,
-- each sending `sent`. It gives every hop made (its URL and the headers sent)
-- and how the request ended. An empty list of answers is an exchange that
-- failed.
def follow (proxyAuth : Url → Option Nat) : Nat → Parts → List Answer → List (Url × Headers) × Outcome
  -- No exchange left: too many redirects;
  | 0, _, _ => ([], .tooManyRedirects)
  -- otherwise send this hop, and look at the answer.
  | n + 1, p, answers =>
    match answers with
    | .redirect code to :: rest =>
      -- A redirect: this hop, then the hops of the request it asks for;
      ((p.url, sent proxyAuth p) :: (follow proxyAuth n (redirect p code to) rest).1,
        (follow proxyAuth n (redirect p code to) rest).2)
    -- the response: this hop, and the request is done;
    | .final :: _ => ([(p.url, sent proxyAuth p)], .response p.url)
    -- anything else: this hop, and the request failed.
    | _ => ([(p.url, sent proxyAuth p)], .error)

-- `REDIRECT_LIMIT`: how many redirects a request follows.
def REDIRECT_LIMIT : Nat := 10

-- `Client::execute` with any redirect limit: the defaults merged once, then
-- `limit + 1` exchanges at most (`for _ in 0..=REDIRECT_LIMIT`).
def executeWith (limit : Nat) (proxyAuth : Url → Option Nat) (defaults : Headers)
    (request : Parts) (answers : List Answer) : List (Url × Headers) × Outcome :=
  follow proxyAuth (limit + 1) { request with headers := withDefaults defaults request.headers } answers

-- `Client::execute` as shipped.
def execute (proxyAuth : Url → Option Nat) (defaults : Headers) (request : Parts)
    (answers : List Answer) : List (Url × Headers) × Outcome :=
  executeWith REDIRECT_LIMIT proxyAuth defaults request answers

-- Where a value may be sent: one the caller gave, only to the request's own
-- URL or its origin; one the client added for a proxy, only on its own hop.
def MeantFor (home : Url) (v : Value) (to : Url) : Prop :=
  match v.source with
  | .proxy hop => hop = to
  | _ => to = home ∨ SameOrigin home to

-- Whether a value may go somewhere can be checked.
instance (home : Url) (v : Value) (to : Url) : Decidable (MeantFor home v to) := by
  unfold MeantFor; split <;> infer_instance

-- A value the caller gave: on the request, or as a default.
def Caller (v : Value) : Prop := v.source = .request ∨ v.source = .default

-- Promise: what remains after a removal was there before.
theorem mem_remove {h : Headers} {n : Name} {e : Name × Value} (he : e ∈ remove h n) :
    e ∈ h ∧ e.1 ≠ n := by
  -- A removal is a filter: keep the member, and the filter's test.
  unfold remove at he
  have := List.mem_filter.mp he
  exact ⟨this.1, of_decide_eq_true this.2⟩

-- Promise: what remains after removing a list of names was there before, and
-- has none of those names.
theorem mem_removeAll : ∀ (ns : List Name) {h : Headers} {e : Name × Value},
    e ∈ removeAll h ns → e ∈ h ∧ e.1 ∉ ns
  -- No names: the headers are unchanged.
  | [], _, _, he => ⟨he, by simp⟩
  -- One name, then the rest: undo the rest, then the one.
  | n :: ns, h, e, he => by
    -- Removing n :: ns is removing n first, then the rest.
    unfold removeAll at he
    simp only [List.foldl] at he
    -- The rest: e was there after n went, and is none of the rest.
    have hrest := mem_removeAll ns (h := remove h n) he
    -- n: e was there before, and is not n.
    have hone := mem_remove hrest.1
    -- So e was in h, and is none of n :: ns.
    refine ⟨hone.1, ?_⟩
    simp only [List.mem_cons, not_or]
    exact ⟨hone.2, hrest.2⟩

-- Promise: a header kept by a removal of other names is still there.
theorem mem_removeAll_of {ns : List Name} : ∀ {h : Headers} {e : Name × Value},
    e ∈ h → e.1 ∉ ns → e ∈ removeAll h ns := by
  -- Walk the names one at a time, keeping the header each time.
  induction ns with
  -- No names: nothing is removed.
  | nil => intro h e he _; exact he
  | cons n ns ih =>
    intro h e he hn
    -- e is not n, and none of the rest.
    simp only [List.mem_cons, not_or] at hn
    -- Removing n :: ns is removing n first, then the rest.
    unfold removeAll
    simp only [List.foldl]
    -- The rest keeps e if removing n kept it,
    apply ih _ hn.2
    -- and removing n keeps e, since e is not n.
    unfold remove
    exact List.mem_filter.mpr ⟨he, decide_eq_true hn.1⟩

-- Promise: an inserted header is the new one or one that was there.
theorem mem_insert {h : Headers} {n : Name} {v : Value} {e : Name × Value}
    (he : e ∈ insert h n v) : e = (n, v) ∨ e ∈ h := by
  -- The new one is at the front; the rest is a removal.
  unfold insert at he
  rcases List.mem_cons.mp he with h1 | h1
  -- At the front: the new one.
  · exact Or.inl h1
  -- Behind it: one the removal kept, so one that was there.
  · exact Or.inr (mem_remove h1).1

-- Promise: every header after the defaults are merged is a default or the
-- request's own.
theorem mem_withDefaults : ∀ (defaults : List (Name × Value)) {acc : Headers} {e : Name × Value},
    e ∈ defaults.foldl (fun acc e => if has acc e.1 then acc else insert acc e.1 e.2) acc →
      e ∈ defaults ∨ e ∈ acc
  -- No defaults: the request's own headers.
  | [], _, _, he => Or.inr he
  -- One default, then the rest.
  | d :: ds, acc, e, he => by
    -- Merging d :: ds is merging d, then the rest.
    simp only [List.foldl] at he
    rcases mem_withDefaults ds he with h1 | h1
    -- From the later defaults: a default.
    · exact Or.inl (List.mem_cons_of_mem d h1)
    -- From the headers after this default: it, or what was there.
    · split at h1
      -- The request already had d's name: d was skipped, e was there.
      · exact Or.inr h1
      -- d was inserted: e is d, or was there.
      · rcases mem_insert h1 with h2 | h2
        · exact Or.inl (by rw [h2]; exact List.mem_cons_self _ _)
        · exact Or.inr h2

-- What `follow` keeps true from hop to hop: every header on the request was
-- given by the caller, and while one of them is a credential, the request's
-- URL is still home or of home's origin.
def Good (home : Url) (p : Parts) : Prop :=
  ∀ e ∈ p.headers, Caller e.2 ∧ (Credential e.1 → p.url = home ∨ SameOrigin home p.url)

-- Promise: a redirect's request has no header the request before it lacked.
theorem redirect_headers_sub (p : Parts) (code : Code) (to : Url) :
    ∀ e ∈ (redirect p code to).headers, e ∈ p.headers := by
  intro e he
  -- Look at which rules applied: each one only removes headers.
  unfold redirect at he
  simp only at he
  -- Turned into a GET or not, stripped or not: four cases, each a removal of
  -- nothing, of some names, or of some names twice.
  split at he <;> split at he <;>
    first
    | exact he
    | exact (mem_removeAll _ he).1
    | exact (mem_removeAll _ (mem_removeAll _ he).1).1

-- Promise: a redirect to another origin carries no credential.
theorem cross_origin_redirect_strips_credentials (p : Parts) (code : Code) (to : Url)
    -- Given the target's origin differs from this hop's,
    (h : ¬ SameOrigin to p.url) :
    -- no header of the next request is a credential.
    ∀ e ∈ (redirect p code to).headers, ¬ Credential e.1 := by
  intro e he hc
  -- The credentials were removed, and e is a credential: impossible.
  unfold redirect at he
  -- Another origin: the stripping applied.
  simp only [h, if_false] at he
  -- e survived the stripping, so its name is not stripped; but it is.
  exact (mem_removeAll _ he).2 (credential_stripped hc)

-- Promise: a redirect within the origin keeps every credential, exactly.
theorem same_origin_redirect_keeps_credentials (p : Parts) (code : Code) (to : Url)
    -- Given the target has this hop's origin,
    (h : SameOrigin to p.url) :
    -- a credential is on the next request exactly when it is on this one.
    ∀ e : Name × Value, Credential e.1 → (e ∈ (redirect p code to).headers ↔ e ∈ p.headers) := by
  intro e hc
  -- On the next request: it was on this one, as for any header;
  refine ⟨fun he => redirect_headers_sub p code to e he, fun he => ?_⟩
  -- on this one: nothing is stripped; only the content headers may go, and e
  -- is not one.
  unfold redirect
  simp only [h, if_true]
  split
  -- Turned into a GET: the content headers went, e stayed.
  · exact mem_removeAll_of he (credential_not_content hc)
  -- Not turned: every header stayed.
  · exact he

-- Promise: `follow` keeps `Good` across a redirect.
theorem good_redirect {home : Url} {p : Parts} (hp : Good home p) (code : Code) (to : Url) :
    Good home (redirect p code to) := by
  intro e he
  -- e was on the request before, so it was given by the caller, and home
  -- held for it there;
  have hold := hp e (redirect_headers_sub p code to e he)
  -- it is still given by the caller; what is left is where it may be.
  refine ⟨hold.1, fun hc => ?_⟩
  -- The next URL is `to`.
  have hurl : (redirect p code to).url = to := by
    unfold redirect; simp only
  rw [hurl]
  -- Is `to` of this hop's origin?
  by_cases hs : SameOrigin to p.url
  -- Within the origin: home's origin carries over, through this hop.
  · rcases hold.2 hc with hh | hh
    -- This hop was home itself: `to` is of home's origin.
    · exact Or.inr (by rw [← hh]; exact hs.symm)
    -- This hop was of home's origin: so is `to`.
    · exact Or.inr (hh.trans hs.symm)
  -- To another origin: the credential was stripped, so it is not here.
  · exact absurd hc (cross_origin_redirect_strips_credentials p code to hs e he)

-- Promise: what one hop sends is meant where it goes, for a `Good` request.
theorem good_sent {home : Url} {p : Parts} (hp : Good home p) (proxyAuth : Url → Option Nat) :
    ∀ e ∈ sent proxyAuth p, Credential e.1 → MeantFor home e.2 p.url := by
  intro e he hc
  -- A header the request carries: given by the caller, so meant for home.
  have carried : e ∈ p.headers → MeantFor home e.2 p.url := by
    intro hin
    -- The request is `Good`: e was given, and home holds for it.
    have hg := hp e hin
    -- Given on the request or as a default: meant for home's origin.
    rcases hg.1 with hs | hs <;> (unfold MeantFor; rw [hs]; exact hg.2 hc)
  -- What the hop sends: the request's headers, maybe with the proxy's.
  unfold sent at he
  split at he
  -- Plain HTTP with no Proxy-Authorization of its own: the proxy may add one.
  · split at he
    -- The proxy's credential, tagged for this hop, or a header carried.
    · rcases mem_insert he with h1 | h1
      · subst h1; unfold MeantFor; rfl
      · exact carried h1
    -- No proxy credential: only the request's headers.
    · exact carried he
  -- Otherwise only the request's headers.
  · exact carried he

-- Promise: every hop of `follow` sends credentials only where they are meant.
theorem follow_good (proxyAuth : Url → Option Nat) (home : Url) :
    ∀ (n : Nat) (p : Parts) (answers : List Answer), Good home p →
      ∀ hop ∈ (follow proxyAuth n p answers).1, ∀ e ∈ hop.2, Credential e.1 →
        MeantFor home e.2 hop.1 := by
  intro n
  induction n with
  -- No exchange: no hop.
  | zero => intro p answers _ hop hhop; simp [follow] at hhop
  | succ n ih =>
    intro p answers hp hop hhop
    -- This hop is `Good`; the later ones follow from the redirect's request.
    have here : ∀ e ∈ sent proxyAuth p, Credential e.1 → MeantFor home e.2 p.url :=
      good_sent hp proxyAuth
    unfold follow at hhop
    split at hhop
    -- A redirect: this hop, or a hop of the next request, which is `Good`.
    · rcases List.mem_cons.mp hhop with h1 | h1
      · subst h1; exact here
      · exact ih _ _ (good_redirect hp _ _) hop h1
    -- The response: this hop only.
    · simp only [List.mem_singleton] at hhop; subst hhop; exact here
    -- An error: this hop only.
    · simp only [List.mem_singleton] at hhop; subst hhop; exact here

-- Promise: whatever the servers answer, no hop of a request sends a
-- credential where it is not meant: a credential the caller gave (on the
-- request or as a default) only to the request's own URL or origin, and a
-- proxy credential the client added only on its own hop.
theorem credentials_stay_with_their_origin (limit : Nat) (proxyAuth : Url → Option Nat)
    (defaults : Headers) (request : Parts) (answers : List Answer)
    -- Given every default is tagged as a default,
    (hd : ∀ e ∈ defaults, e.2.source = .default)
    -- and every header of the request as the request's,
    (hr : ∀ e ∈ request.headers, e.2.source = .request) :
    -- every credential on every hop is meant for where it goes.
    ∀ hop ∈ (executeWith limit proxyAuth defaults request answers).1,
      ∀ e ∈ hop.2, Credential e.1 → MeantFor request.url e.2 hop.1 := by
  -- Every hop of `follow` is fine once the first request is `Good`.
  apply follow_good
  -- The first request is `Good`: every header was given, and it is home.
  intro e he
  rcases mem_withDefaults defaults he with h1 | h1
  -- A default: given as one, and the first hop is home.
  · exact ⟨Or.inr (hd e h1), fun _ => Or.inl rfl⟩
  -- The request's own: given on it, and the first hop is home.
  · exact ⟨Or.inl (hr e h1), fun _ => Or.inl rfl⟩

-- Promise: `follow` with `n` exchanges left makes at most `n` of them.
theorem follow_length_le (proxyAuth : Url → Option Nat) :
    ∀ (n : Nat) (p : Parts) (answers : List Answer), (follow proxyAuth n p answers).1.length ≤ n := by
  intro n
  induction n with
  -- None left: no hop.
  | zero => intro p answers; simp [follow]
  | succ n ih =>
    intro p answers
    -- Look at the first answer.
    unfold follow
    split
    -- A redirect: one hop more than the rest, which is at most `n`.
    · simp only [List.length_cons]; exact Nat.succ_le_succ (ih _ _)
    -- The response or an error: one hop.
    · simp
    · simp

-- Promise: one request makes at most `limit + 1` exchanges: the first and
-- `limit` redirects.
theorem at_most_limit_plus_one_exchanges (limit : Nat) (proxyAuth : Url → Option Nat)
    (defaults : Headers) (request : Parts) (answers : List Answer) :
    (executeWith limit proxyAuth defaults request answers).1.length ≤ limit + 1 :=
  follow_length_le proxyAuth _ _ _

-- Every answer of the list is a redirect.
def AllRedirects (answers : List Answer) : Prop :=
  ∀ a ∈ answers, ∃ code to, a = .redirect code to

-- Promise: when the servers answer every exchange with a redirect, the
-- request fails with too many redirects, after exactly `n` exchanges.
theorem endless_redirects_fail (proxyAuth : Url → Option Nat) :
    ∀ (n : Nat) (p : Parts) (answers : List Answer),
      -- Given at least `n` answers, all redirects,
      n ≤ answers.length → AllRedirects answers →
      -- the request makes `n` exchanges and fails with too many redirects.
      (follow proxyAuth n p answers).2 = .tooManyRedirects ∧
        (follow proxyAuth n p answers).1.length = n := by
  intro n
  induction n with
  -- None left: no exchange, and too many redirects.
  | zero => intro p answers _ _; simp [follow]
  | succ n ih =>
    intro p answers hlen hall
    -- The first answer exists, and is a redirect.
    match answers, hlen, hall with
    | a :: rest, hlen, hall =>
      obtain ⟨code, to, ha⟩ := hall a (List.mem_cons_self _ _)
      subst ha
      -- The rest are redirects too,
      have hrest : AllRedirects rest := fun b hb => hall b (List.mem_cons_of_mem _ hb)
      -- and there are at least `n` of them,
      have hl : n ≤ rest.length := by simp at hlen; omega
      -- so the next request makes `n` exchanges and fails;
      have := ih (redirect p code to) rest hl hrest
      -- with this one, `n + 1` exchanges, and the same failure.
      simp only [follow, List.length_cons]
      exact ⟨this.1, by rw [this.2]⟩

-- Promise, as shipped: a request makes at most eleven exchanges.
theorem shipped_limit (proxyAuth : Url → Option Nat) (defaults : Headers) (request : Parts)
    (answers : List Answer) :
    (execute proxyAuth defaults request answers).1.length ≤ 11 :=
  at_most_limit_plus_one_exchanges REDIRECT_LIMIT proxyAuth defaults request answers

/-! ## What each guard prevents

Each witness below takes one guard away and shows, on one concrete chain, the
promise it breaks. -/

-- RED (the reviewed bug): the default headers added on every hop, inside the
-- exchange, after `redirect` stripped them.
def followPerHop (proxyAuth : Url → Option Nat) (defaults : Headers) :
    Nat → Parts → List Answer → List (Url × Headers) × Outcome
  -- No exchange left: too many redirects;
  | 0, _, _ => ([], .tooManyRedirects)
  | n + 1, p, answers =>
    -- what this hop sends: the defaults merged into it again;
    let hop := (p.url, sent proxyAuth { p with headers := withDefaults defaults p.headers })
    match answers with
    -- a redirect: this hop, then the next request's;
    | .redirect code to :: rest =>
      (hop :: (followPerHop proxyAuth defaults n (redirect p code to) rest).1,
        (followPerHop proxyAuth defaults n (redirect p code to) rest).2)
    -- the response, or an error: this hop only.
    | .final :: _ => ([hop], .response p.url)
    | _ => ([hop], .error)

-- Home: https on host 1, port 443. Elsewhere: https on host 2.
def home : Url := ⟨.https, 1, 443, 0⟩
def elsewhere : Url := ⟨.https, 2, 443, 0⟩

-- A request home with no headers of its own, and an Authorization default.
def bare : Parts := ⟨.get, home, [], false⟩
def authDefault : Headers := [(.authorization, ⟨.default, 7⟩)]

-- Witness: with the defaults merged on every hop, a 302 to another host
-- sends the default Authorization there.
theorem per_hop_defaults_leak :
    ∃ hop ∈ (followPerHop (fun _ => none) authDefault 11 bare
        [.redirect .found elsewhere, .final]).1,
      ∃ e ∈ hop.2, Credential e.1 ∧ ¬ MeantFor home e.2 hop.1 := by
  -- Run the two hops and look: the second, to host 2, carries the default.
  decide

-- And the shipped code, on the same chain, sends it home only.
theorem shipped_defaults_stay_home :
    ∀ hop ∈ (execute (fun _ => none) authDefault bare [.redirect .found elsewhere, .final]).1,
      ∀ e ∈ hop.2, Credential e.1 → MeantFor home e.2 hop.1 := by
  -- Run the two hops and look: the second carries nothing.
  decide

-- RED: origins compared by host alone, ignoring scheme and port.
def SameHost (a b : Url) : Prop := a.host = b.host

-- That comparison is a plain one too.
instance (a b : Url) : Decidable (SameHost a b) := by
  unfold SameHost; infer_instance

-- `redirect` with that comparison.
def redirectHostOnly (p : Parts) (code : Code) (to : Url) : Parts :=
  -- The method rules, as shipped;
  let q : Parts :=
    if toGet p.method code then
      { p with method := .get, body := false, headers := removeAll p.headers contentHeaders }
    else p
  -- the credentials kept whenever the host is the same;
  let r : Parts := if SameHost to p.url then q else { q with headers := removeAll q.headers stripped }
  -- and the new URL.
  { r with url := to }

-- The same host over plain HTTP, and on another port.
def downgraded : Url := ⟨.http, 1, 80, 0⟩
def otherPort : Url := ⟨.https, 1, 8443, 0⟩

-- A request home that carries its own Authorization.
def withAuth : Parts := ⟨.get, home, [(.authorization, ⟨.request, 9⟩)], false⟩

-- Witness: compared by host alone, a redirect from https to plain http on the
-- same host keeps the Authorization, and sends it in the clear to an origin
-- that is not home's.
theorem host_only_downgrade_leaks :
    ∃ e ∈ (redirectHostOnly withAuth .found downgraded).headers,
      Credential e.1 ∧ ¬ MeantFor home e.2 downgraded := by
  -- The next request still carries the Authorization, and http is not https.
  decide

-- Witness: compared by host alone, a redirect to another port on the same
-- host (another service) keeps the Authorization.
theorem host_only_port_leaks :
    ∃ e ∈ (redirectHostOnly withAuth .temporary otherPort).headers,
      Credential e.1 ∧ ¬ MeantFor home e.2 otherPort := by
  -- The next request still carries the Authorization, and 8443 is not 443.
  decide

-- And the shipped `redirect` strips both.
theorem shipped_strips_downgrade_and_port :
    (redirect withAuth .found downgraded).headers = [] ∧
      (redirect withAuth .temporary otherPort).headers = [] := by
  -- Both next requests carry no header at all.
  decide

-- RED: no redirect limit, as many exchanges as the servers ask for (one more
-- than the answers, so the budget never runs out first).
def followUnlimited (proxyAuth : Url → Option Nat) (p : Parts) (answers : List Answer) :
    List (Url × Headers) × Outcome :=
  follow proxyAuth (answers.length + 1) p answers

-- Eleven redirects within home, then the response.
def elevenRedirects : List Answer := List.replicate 11 (.redirect .found home) ++ [.final]

-- Witness: with no limit, eleven redirects make twelve exchanges, past the
-- shipped eleven.
theorem unlimited_runs_past_the_limit :
    (followUnlimited (fun _ => none) bare elevenRedirects).1.length = 12 ∧
      REDIRECT_LIMIT + 1 < 12 := by
  -- Count the hops: eleven redirects and the response.
  decide

-- And the shipped client stops at eleven exchanges and fails.
theorem shipped_stops_at_the_limit :
    (execute (fun _ => none) [] bare elevenRedirects).1.length = 11 ∧
      (execute (fun _ => none) [] bare elevenRedirects).2 = .tooManyRedirects := by
  -- Count the hops: eleven, and then no exchange is left.
  decide

end HttpClient
