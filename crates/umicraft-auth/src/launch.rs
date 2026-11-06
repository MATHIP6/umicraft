// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reads the launcher-provided session from arguments or environment.

use crate::{error::AuthError, secret::Secret, session::Session};

const FLAGS: [&str; 3] = ["--username", "--uuid", "--accessToken"];
const ENV_VARS: [&str; 3] = [
    "UMICRAFT_USERNAME",
    "UMICRAFT_UUID",
    "UMICRAFT_ACCESS_TOKEN",
];

/// Splits `arg` into `(flag index, inline value)` if it is one of the session
/// flags, in either `--flag` or `--flag=value` form.
fn match_flag(arg: &str) -> Option<(usize, Option<&str>)> {
    let (name, value) = match arg.split_once('=') {
        Some((n, v)) => (n, Some(v)),
        None => (arg, None),
    };
    FLAGS.iter().position(|f| *f == name).map(|i| (i, value))
}

/// Builds the session from `args` (vanilla launcher flags), falling back to
/// the `UMICRAFT_USERNAME`, `UMICRAFT_UUID` and `UMICRAFT_ACCESS_TOKEN`
/// variables read through `env`. Arguments win over the environment.
///
/// Returns `Ok(None)` when none of the three values is present (offline).
///
/// # Errors
/// [`AuthError::IncompleteSession`] when only some are present (the error
/// names the missing flags, never values), or the validation errors of
/// [`Session::new`].
pub fn session_from_launch(
    args: &[String],
    env: impl Fn(&str) -> Option<String>,
) -> Result<Option<Session>, AuthError> {
    let mut values: [Option<String>; 3] = [None, None, None];
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some((i, inline)) = match_flag(arg) {
            values[i] = inline.map(str::to_owned).or_else(|| iter.next().cloned());
        }
    }
    for (value, var) in values.iter_mut().zip(ENV_VARS) {
        if value.is_none() {
            *value = env(var);
        }
    }
    let [Some(name), Some(uuid), Some(token)] = &values else {
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let missing = FLAGS
            .iter()
            .zip(&values)
            .filter(|(_, v)| v.is_none())
            .map(|(f, _)| *f)
            .collect();
        return Err(AuthError::IncompleteSession { missing });
    };
    Session::new(name, uuid, Secret::new(token.as_str())).map(Some)
}

/// Returns `args` without the session flags and their values, so other
/// argument parsers do not choke on them (and never see the token).
pub fn strip_session_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match match_flag(arg) {
            Some((_, None)) => {
                iter.next();
            }
            Some((_, Some(_))) => {}
            None => out.push(arg.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn all_absent_is_offline() {
        assert_eq!(
            session_from_launch(&args(&["--fullscreen"]), no_env).unwrap(),
            None
        );
    }

    #[test]
    fn reads_space_and_equals_forms() {
        let a = args(&[
            "--username",
            "Notch",
            &format!("--uuid={UUID}"),
            "--accessToken",
            "tok",
        ]);
        let s = session_from_launch(&a, no_env).unwrap().unwrap();
        assert_eq!((s.username.as_str(), s.uuid.as_str()), ("Notch", UUID));
        assert_eq!(s.access_token.expose(), "tok");
    }

    #[test]
    fn falls_back_to_env_and_args_win() {
        let env = |k: &str| match k {
            "UMICRAFT_USERNAME" => Some("EnvUser".to_owned()),
            "UMICRAFT_UUID" => Some(UUID.to_owned()),
            "UMICRAFT_ACCESS_TOKEN" => Some("envtok".to_owned()),
            _ => None,
        };
        let s = session_from_launch(&args(&["--username", "ArgUser"]), env)
            .unwrap()
            .unwrap();
        assert_eq!(s.username, "ArgUser");
        assert_eq!(s.access_token.expose(), "envtok");
    }

    #[test]
    fn partial_names_missing_flags_without_values() {
        let err = session_from_launch(
            &args(&["--username", "Notch", "--accessToken", "sekret"]),
            no_env,
        )
        .unwrap_err();
        assert!(matches!(&err, AuthError::IncompleteSession { missing } if missing == &["--uuid"]));
        assert!(!err.to_string().contains("sekret"));
    }

    #[test]
    fn strips_flags_and_values() {
        let a = args(&[
            "--w",
            "--username",
            "N",
            "--uuid=x",
            "--accessToken",
            "t",
            "--h",
        ]);
        assert_eq!(strip_session_args(&a), args(&["--w", "--h"]));
    }
}
