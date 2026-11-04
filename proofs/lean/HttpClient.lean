-- A credential stays with the origin it was meant for, across every redirect, within the limit.
import HttpClient.Redirect
-- A body stream that timed out gives its end, and nothing more.
import HttpClient.BodyDeadline
