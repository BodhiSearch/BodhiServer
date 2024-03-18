pub static DEFAULT_PORT: u16 = 7735;
// TODO: see if can use lazy_static to not duplicate port
pub static DEFAULT_PORT_STR: &str = "7735";
pub static DEFAULT_HOST: &str = "127.0.0.1";

pub fn port_from_env_vars(port: Result<String, std::env::VarError>) -> u16 {
  match port {
    Ok(port) => match port.parse::<u16>() {
      Ok(port) => port,
      Err(err) => {
        tracing::debug!(
          err = ?err,
          port = port,
          default_port = DEFAULT_PORT,
          "error parsing port set in environment variable, using default port",
        );
        DEFAULT_PORT
      }
    },
    Err(err) => {
      tracing::debug!(
        err = ?err,
        default_port = DEFAULT_PORT,
        "error reading port from environment variable, using default port",
      );
      DEFAULT_PORT
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{port_from_env_vars, DEFAULT_PORT};
  use rstest::rstest;

  #[test]
  pub fn test_port_from_env_vars_not_present() {
    let port = port_from_env_vars(Err(std::env::VarError::NotPresent));
    assert_eq!(port, DEFAULT_PORT);
  }

  #[test]
  pub fn test_port_from_env_vars_valid() {
    let port = port_from_env_vars(Ok("8055".to_string()));
    assert_eq!(port, 8055);
  }

  #[rstest]
  #[case("notu16")]
  #[case("65536")]
  #[case("-1")]
  pub fn test_port_from_env_vars_malformed(#[case] input: &str) {
    let port = port_from_env_vars(Ok(input.to_string()));
    assert_eq!(port, DEFAULT_PORT);
  }
}
