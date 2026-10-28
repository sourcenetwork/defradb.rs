---------------------------- MODULE EndpointBind ----------------------------
\* A caller selects an address or port before the endpoint opens sockets.
\* Explicit addresses replace preset listeners; optional IPv6 may be unavailable.
\* Rust: crates/p2p/src/iroh/endpoint_config.rs::apply_bind_config.
EXTENDS Naturals, FiniteSets, TLC
\* Each negative configuration plants one concrete configuration mistake.
CONSTANT Bug
\* Requests, environment, configuration and observed sockets have distinct owners.
VARIABLES mode, requestedIp, port, ipv6Available, gatewayAvailable, phase,
          configured, bound, mapping, gatewaySockets
\* Stuttering includes every observed and configured value.
vars == <<mode, requestedIp, port, ipv6Available, gatewayAvailable, phase,
          configured, bound, mapping, gatewaySockets>>
\* The Minimal preset requires IPv4 and makes wildcard IPv6 optional.
Wildcards(p) == {[ip |-> "any4", port |-> p, required |-> TRUE],
                 [ip |-> "any6", port |-> p, required |-> FALSE]}
\* This finite environment distinguishes unavailable IPv6 from a scope violation.
IsV6(ip) == ip \in {"any6", "loop6"}
\* Every modeled request starts before configuration and before any socket opens.
Init == /\ mode \in {"explicit", "port", "default"}
        /\ requestedIp \in {"none", "any4", "any6", "loop4", "loop6", "lan4", "public4"}
        /\ (mode = "explicit" <=> requestedIp # "none")
        /\ port \in {0, 9}
        /\ (mode = "default" => port = 0)
        /\ ipv6Available \in BOOLEAN
        /\ gatewayAvailable \in BOOLEAN
        /\ phase = "new"
        /\ configured = {}
        /\ bound = {}
        /\ mapping = TRUE
        /\ gatewaySockets = {}
\* Selection owns the entire socket set, including its exact IP and requested port.
Selected == IF mode = "explicit"
            THEN {[ip |-> requestedIp, port |-> port, required |-> TRUE]}
            ELSE Wildcards(port)
\* The retain mutation reproduces adding an explicit address to preset sockets.
WithPresets == IF Bug = "retain" /\ mode = "explicit"
               THEN Selected \cup Wildcards(0) ELSE Selected
\* The address mutation widens a specific address into its family's wildcard.
WithAddress == IF Bug = "address" /\ mode = "explicit"
               THEN {[s EXCEPT !.ip = IF IsV6(s.ip) THEN "any6" ELSE "any4"]
                     : s \in WithPresets} ELSE WithPresets
\* The port mutation substitutes ephemeral ports for a concrete caller selection.
WithPort == IF Bug = "port" /\ port # 0
            THEN {[s EXCEPT !.port = 0] : s \in WithAddress} ELSE WithAddress
\* The defaults mutation discards an available preset family without a request.
Plan == IF Bug = "defaults" /\ mode = "default"
        THEN {s \in WithPort : ~IsV6(s.ip)} ELSE WithPort
\* Iroh's gateway mapper follows the default route, not a specific bound interface.
MappingAllowed == mode # "explicit" \/ requestedIp \in {"any4", "any6"}
\* One mutation leaks gateway discovery; the other disables ordinary NAT traversal.
MappingPlan == IF Bug = "mapping-scope" THEN TRUE
               ELSE IF Bug = "mapping-defaults" THEN FALSE ELSE MappingAllowed
\* Configuration settles before binding begins; no intermediate set is observed.
Configure == /\ phase = "new"
             /\ configured' = Plan
             /\ mapping' = MappingPlan
             /\ phase' = "configured"
             /\ UNCHANGED <<mode, requestedIp, port, ipv6Available,
                            gatewayAvailable, bound, gatewaySockets>>
\* A required unavailable family refuses the endpoint rather than widening scope.
Unavailable == \E s \in configured : s.required /\ IsV6(s.ip) /\ ~ipv6Available
\* The OS chooses a nonzero ephemeral port; concrete ports are preserved.
Open(s) == [s EXCEPT !.port = IF s.port = 0 THEN 7 ELSE s.port]
\* Binding observes only selected sockets whose address families are available.
Bind == /\ phase = "configured"
        /\ phase' = IF Unavailable THEN "refused" ELSE "bound"
        /\ bound' = IF Unavailable THEN {}
                     ELSE {Open(s) : s \in {c \in configured : ipv6Available \/ ~IsV6(c.ip)}}
        /\ gatewaySockets' = IF ~Unavailable /\ mapping /\ gatewayAvailable
                              THEN {"any4"} ELSE {}
        /\ UNCHANGED <<mode, requestedIp, port, ipv6Available,
                       gatewayAvailable, configured, mapping>>
\* Settled endpoints need no further action in this configuration model.
Next == Configure \/ Bind
\* Successful explicit binding has exactly one socket, with no retained preset.
ExplicitOnly == phase = "bound" /\ mode = "explicit" => Cardinality(bound) = 1
\* An explicit address cannot become a wildcard or another address family.
ExactAddress == phase = "bound" /\ mode = "explicit" => \A s \in bound : s.ip = requestedIp
\* A concrete port is honored on every socket that successfully opens.
RequestedPort == phase = "bound" /\ port # 0 => \A s \in bound : s.port = port
\* Default binding retains both families when the environment supports both.
DefaultPreserved == phase = "bound" /\ mode = "default" /\ ipv6Available =>
                      {s.ip : s \in bound} = {"any4", "any6"}
\* An unavailable explicit IPv6 address yields a refusal with no open listener.
UnavailableRefused == mode = "explicit" /\ IsV6(requestedIp) /\ ~ipv6Available /\
                      phase \in {"bound", "refused"} => phase = "refused" /\ bound = {}
\* A specific listener never starts unscoped gateway mapping after configuration.
MappingScope == phase # "new" /\ ~MappingAllowed => ~mapping /\ gatewaySockets = {}
\* Wildcard and unspecified configurations keep their ordinary NAT-mapping capability.
MappingDefaultsPreserved == phase # "new" /\ MappingAllowed => mapping
\* The configuration and binding actions preserve these safety promises.
Spec == Init /\ [][Next]_vars
=============================================================================
