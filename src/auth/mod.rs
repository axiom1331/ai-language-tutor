use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    TokenCreation,
    InvalidToken,
    MissingSecret,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidCredentials => write!(f, "Invalid credentials"),
            AuthError::TokenCreation => write!(f, "Failed to create token"),
            AuthError::InvalidToken => write!(f, "Invalid token"),
            AuthError::MissingSecret => write!(f, "JWT secret not configured"),
        }
    }
}

impl std::error::Error for AuthError {}

pub fn verify_credentials(username: &str, password: &str) -> Result<(), AuthError> {
    let expected_username = env::var("AUTH_USERNAME").map_err(|_| AuthError::InvalidCredentials)?;
    let expected_password = env::var("AUTH_PASSWORD").map_err(|_| AuthError::InvalidCredentials)?;

    if username == expected_username && password == expected_password {
        Ok(())
    } else {
        Err(AuthError::InvalidCredentials)
    }
}

pub fn create_jwt(username: &str) -> Result<String, AuthError> {
    let secret = env::var("JWT_SECRET").map_err(|_| AuthError::MissingSecret)?;
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .ok_or(AuthError::TokenCreation)?
        .timestamp() as usize;

    let claims = Claims {
        sub: username.to_string(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::TokenCreation)
}

pub fn verify_jwt(token: &str) -> Result<Claims, AuthError> {
    let secret = env::var("JWT_SECRET").map_err(|_| AuthError::MissingSecret)?;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AuthError::InvalidToken)?;

    Ok(token_data.claims)
}
