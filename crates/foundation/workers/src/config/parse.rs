//! The pool's settings read from text: a config file, a flag, an environment
//! variable. Each `FromStr` accepts what its `Display`-like form in the
//! start-up report names, so a value read back from the report parses again.
//!
//! | Setting | Accepted |
//! |---|---|
//! | [`Workers`] | `auto`, or a count of at least 1 |
//! | [`Placement`] | `unpinned`, `spread`, `pack`, or CPU lists split by `;` (`0-3;4-7`) |
//! | [`Smt`] | `physical`, `all` |
//! | [`MemoryPolicy`] | `off`, `local`, `bind:<nodes>` (`bind:0,1`) |
//! | [`MemoryLock`] | `off`, `current`, `future`, each `future` form with `+on-fault` |
//! | [`Priority`] | `inherit`, `rr:<1-99>`, `fifo:<1-99>`, `nice:<-20..19>`, a macOS QoS class name |

use std::str::FromStr;

use super::{
    MemoryLock, MemoryPolicy, Placement, Priority, QosClass, RealTimePolicy, Smt, Workers,
};
use crate::topology::CpuSet;
use crate::Error;

fn invalid(setting: &str, value: &str, accepted: &str) -> Error {
    Error::Config(format!("{setting} {value:?} is not one of: {accepted}"))
}

impl FromStr for Workers {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        match value.trim() {
            "auto" => Ok(Workers::Auto),
            count => match count.parse::<usize>() {
                Ok(n) if n > 0 => Ok(Workers::Count(n)),
                _ => Err(invalid("workers", value, "auto, or a count of at least 1")),
            },
        }
    }
}

impl FromStr for Placement {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        match value.trim() {
            "unpinned" => Ok(Placement::Unpinned),
            "spread" => Ok(Placement::Spread),
            "pack" => Ok(Placement::Pack),
            sets => sets
                .split(';')
                .map(|set| CpuSet::parse(set.trim()))
                .collect::<Result<Vec<_>, _>>()
                .map(Placement::Custom)
                .map_err(|_| {
                    invalid(
                        "placement",
                        value,
                        "unpinned, spread, pack, or CPU lists split by ';'",
                    )
                }),
        }
    }
}

impl FromStr for Smt {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        match value.trim() {
            "physical" => Ok(Smt::PhysicalOnly),
            "all" => Ok(Smt::All),
            _ => Err(invalid("smt", value, "physical, all")),
        }
    }
}

impl FromStr for MemoryPolicy {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        let trimmed = value.trim();
        match trimmed {
            "off" => return Ok(MemoryPolicy::Off),
            "local" => return Ok(MemoryPolicy::Local),
            _ => {}
        }
        trimmed
            .strip_prefix("bind:")
            .and_then(|nodes| {
                nodes
                    .split(',')
                    .map(|node| node.trim().parse::<usize>().ok())
                    .collect::<Option<Vec<_>>>()
            })
            .filter(|nodes| !nodes.is_empty())
            .map(MemoryPolicy::Bind)
            .ok_or_else(|| invalid("memory policy", value, "off, local, bind:<nodes>"))
    }
}

impl FromStr for MemoryLock {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        let (base, on_fault) = match value.trim().strip_suffix("+on-fault") {
            Some(base) => (base, true),
            None => (value.trim(), false),
        };
        let future = match base {
            "off" if !on_fault => return Ok(MemoryLock::Off),
            "current" => false,
            "future" => true,
            _ => {
                return Err(invalid(
                    "memory lock",
                    value,
                    "off, current, future, each with +on-fault",
                ))
            }
        };
        Ok(MemoryLock::Lock {
            future,
            on_fault,
            footprint: None,
        })
    }
}

impl FromStr for Priority {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        let accepted = "inherit, rr:<1-99>, fifo:<1-99>, nice:<-20..19>, user-interactive, \
                        user-initiated, default, utility, background";
        let trimmed = value.trim();
        let realtime = |policy, level: &str| match level.parse::<u8>() {
            Ok(priority @ 1..=99) => Ok(Priority::RealTime { policy, priority }),
            _ => Err(invalid("priority", value, accepted)),
        };
        if let Some(level) = trimmed.strip_prefix("rr:") {
            return realtime(RealTimePolicy::RoundRobin, level);
        }
        if let Some(level) = trimmed.strip_prefix("fifo:") {
            return realtime(RealTimePolicy::Fifo, level);
        }
        if let Some(nice) = trimmed.strip_prefix("nice:") {
            return match nice.parse::<i8>() {
                Ok(nice @ -20..=19) => Ok(Priority::Nice(nice)),
                _ => Err(invalid("priority", value, accepted)),
            };
        }
        match trimmed {
            "inherit" => Ok(Priority::Inherit),
            "user-interactive" => Ok(Priority::Qos(QosClass::UserInteractive)),
            "user-initiated" => Ok(Priority::Qos(QosClass::UserInitiated)),
            "default" => Ok(Priority::Qos(QosClass::Default)),
            "utility" => Ok(Priority::Qos(QosClass::Utility)),
            "background" => Ok(Priority::Qos(QosClass::Background)),
            _ => Err(invalid("priority", value, accepted)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accepted_form_parses_and_the_rest_are_refused() {
        assert_eq!("auto".parse::<Workers>(), Ok(Workers::Auto));
        assert_eq!("4".parse::<Workers>(), Ok(Workers::Count(4)));
        assert!("0".parse::<Workers>().is_err());
        assert!("many".parse::<Workers>().is_err());

        assert_eq!("spread".parse::<Placement>(), Ok(Placement::Spread));
        assert_eq!("pack".parse::<Placement>(), Ok(Placement::Pack));
        assert_eq!("unpinned".parse::<Placement>(), Ok(Placement::Unpinned));
        assert_eq!(
            "0-1;2".parse::<Placement>(),
            Ok(Placement::Custom(vec![
                CpuSet::parse("0-1").unwrap(),
                CpuSet::parse("2").unwrap()
            ]))
        );
        assert!("anywhere".parse::<Placement>().is_err());

        assert_eq!("all".parse::<Smt>(), Ok(Smt::All));
        assert!("half".parse::<Smt>().is_err());

        assert_eq!(
            "bind:0,1".parse::<MemoryPolicy>(),
            Ok(MemoryPolicy::Bind(vec![0, 1]))
        );
        assert_eq!("local".parse::<MemoryPolicy>(), Ok(MemoryPolicy::Local));
        assert!("bind:".parse::<MemoryPolicy>().is_err());

        assert_eq!("off".parse::<MemoryLock>(), Ok(MemoryLock::Off));
        assert_eq!(
            "future+on-fault".parse::<MemoryLock>(),
            Ok(MemoryLock::Lock {
                future: true,
                on_fault: true,
                footprint: None
            })
        );
        assert!("off+on-fault".parse::<MemoryLock>().is_err());

        assert_eq!(
            "rr:50".parse::<Priority>(),
            Ok(Priority::RealTime {
                policy: RealTimePolicy::RoundRobin,
                priority: 50
            })
        );
        assert_eq!("nice:-20".parse::<Priority>(), Ok(Priority::Nice(-20)));
        assert_eq!(
            "utility".parse::<Priority>(),
            Ok(Priority::Qos(QosClass::Utility))
        );
        assert!("rr:0".parse::<Priority>().is_err());
        assert!("nice:20".parse::<Priority>().is_err());
    }
}
