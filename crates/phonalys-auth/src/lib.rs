//! Authentification de Phonalys.
//!
//! Le facteur « connaissance » (phrase de passe) ouvre le coffre par cryptographie.
//! Le second facteur (TOTP ici, FIDO2 ensuite) est vérifié avec un secret scellé
//! *dans* le coffre : on ne peut donc pas le contourner via l'application.

pub mod password;
pub mod recovery;
pub mod session;
pub mod totp;

pub use password::{check_passphrase, PassphraseError};
pub use recovery::RecoveryCodes;
pub use session::{SessionConfig, SessionStore};
pub use totp::Totp;
