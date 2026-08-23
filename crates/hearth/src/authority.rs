use crate::error::DomainError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HouseMode {
    Base,
    Full,
    Degraded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthVerdict {
    Healthy,
    Unhealthy { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Authority {
    Base,
    Full,
}

pub fn authorize(mode: HouseMode, health: HealthVerdict) -> Result<Authority, DomainError> {
    match (mode, health) {
        (HouseMode::Full, HealthVerdict::Healthy) => Ok(Authority::Full),
        (HouseMode::Full, HealthVerdict::Unhealthy { reason }) => {
            Err(DomainError::FullUnhealthy { reason })
        }
        (HouseMode::Degraded, _) => Err(DomainError::DegradedUnavailable),
        (HouseMode::Base, _) => Ok(Authority::Base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_unhealthy_never_falls_back_to_base() {
        let result = authorize(
            HouseMode::Full,
            HealthVerdict::Unhealthy {
                reason: "db down".into(),
            },
        );
        assert_eq!(
            result,
            Err(DomainError::FullUnhealthy {
                reason: "db down".into()
            })
        );
        assert_eq!(
            authorize(HouseMode::Degraded, HealthVerdict::Healthy),
            Err(DomainError::DegradedUnavailable)
        );
    }
}
