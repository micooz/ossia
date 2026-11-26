use crate::{AppError, ErrorCode, OmrService, Result, UiPreferences, OSSIA_OMR_URL};

#[derive(Clone)]
pub(crate) struct OmrConfig {
    pub url: String,
    pub token: String,
}

impl OmrConfig {
    pub fn from_preferences(preferences: &UiPreferences) -> Result<Self> {
        if !preferences.omr_enabled() {
            return Err(AppError::new(
                ErrorCode::OmrNotConfigured,
                "Remote OMR is not enabled",
            ));
        }
        let invalid = |message: String| AppError::new(ErrorCode::OmrConfigurationInvalid, message);
        let (url, token) = match preferences.omr_service {
            OmrService::Ossia => (OSSIA_OMR_URL.to_string(), preferences.omr_token.as_deref()),
            OmrService::Custom => {
                let url = preferences
                    .omr_url
                    .as_deref()
                    .ok_or_else(|| invalid("Custom remote OMR URL is not configured".into()))?;
                (
                    crate::preferences::validate_omr_url(url).map_err(|e| invalid(e.message))?,
                    preferences.omr_custom_token.as_deref(),
                )
            }
        };
        let token = token.ok_or_else(|| invalid("Remote OMR token is not configured".into()))?;
        let token =
            crate::preferences::normalize_omr_token(token).map_err(|e| invalid(e.message))?;
        Ok(Self { url, token })
    }
}
