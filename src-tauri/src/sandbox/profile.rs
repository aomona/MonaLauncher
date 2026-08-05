use std::error::Error;
use std::fmt;

const PROFILE_PREFIX: &str = "me.aomona.monalauncher.";
const MAX_PROFILE_NAME_LENGTH: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum ProfileNameError {
    EmptyInstanceId,
    TooLong,
    InvalidCharacter(char),
}

impl fmt::Display for ProfileNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInstanceId => {
                write!(formatter, "instance ID must not be empty")
            }
            Self::TooLong => {
                write!(
                    formatter,
                    "AppContainer profile name must not exceed {MAX_PROFILE_NAME_LENGTH} characters"
                )
            }
            Self::InvalidCharacter(character) => {
                write!(
                    formatter,
                    "instance ID contains an invalid character: {character:?}"
                )
            }
        }
    }
}

impl Error for ProfileNameError {}

pub fn profile_name_for_instance(instance_id: &str) -> Result<String, ProfileNameError> {
    if instance_id.is_empty() {
        return Err(ProfileNameError::EmptyInstanceId);
    }

    for character in instance_id.chars() {
        let allowed = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');

        if !allowed {
            return Err(ProfileNameError::InvalidCharacter(character));
        }
    }

    let profile_name = format!("{PROFILE_PREFIX}{instance_id}");

    if profile_name.len() > MAX_PROFILE_NAME_LENGTH {
        return Err(ProfileNameError::TooLong);
    }

    Ok(profile_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_profile_name_from_valid_instance_id() {
        let result = profile_name_for_instance("sandbox-test");

        assert_eq!(result, Ok("me.aomona.monalauncher.sandbox-test".to_owned()));
    }

    #[test]
    fn rejects_empty_instance_id() {
        let result = profile_name_for_instance("");

        assert_eq!(result, Err(ProfileNameError::EmptyInstanceId));
    }

    #[test]
    fn rejects_path_separator() {
        let result = profile_name_for_instance("../secret");

        assert_eq!(result, Err(ProfileNameError::InvalidCharacter('/')));
    }

    #[test]
    fn rejects_overly_long_profile_name() {
        let instance_id = "a".repeat(64);

        let result = profile_name_for_instance(&instance_id);

        assert_eq!(result, Err(ProfileNameError::TooLong));
    }
}
