use crate::util::log_line;

/// Trusts the embedded signing certificate for package registration.
///
/// - Per-user stores (CurrentUser TrustedPeople + Root): added silently,
///   no elevation — the generic trust baseline.
/// - Machine stores (LocalMachine TrustedPeople + Root): package signature
///   validation runs in the system context and only consults machine
///   stores; without them sparse MSIX registration fails with 0x800B0109.
///   They are only touched when identity registration is actually planned,
///   and require ONE UAC prompt — a declined prompt leaves the per-user
///   stores in place and the later registration reports the real error.
pub fn trust_certificate(cer_bytes: &[u8], subject: &str, identity_needed: bool) -> Result<(), String> {
    if cer_bytes.is_empty() {
        return Ok(());
    }
    let tmp = std::env::temp_dir().join("deploy-identity.cer");
    std::fs::write(&tmp, cer_bytes).map_err(|e| format!("could not stage certificate: {e}"))?;
    let path = tmp.to_string_lossy().to_string();

    for store in ["TrustedPeople", "Root"] {
        let already = crate::util::run_hidden("certutil", &["-user", "-verifystore", store, &path])
            .is_ok();
        if !already {
            crate::util::run_hidden("certutil", &["-user", "-f", "-addstore", store, &path])
                .map_err(|e| {
                    format!("could not trust the signing certificate (CurrentUser\\{store}): {e}")
                })?;
        }
    }

    if identity_needed {
        // `certutil -verifystore` checks chain building, not membership, and
        // reports NTE_NOT_FOUND for a trusted self-signed leaf — so probe the
        // store listing for the subject instead.
        let machine_trusted = |subject: &str| {
            crate::util::run_hidden("certutil", &["-store", "TrustedPeople"])
                .map(|out| out.contains(subject))
                .unwrap_or(false)
        };
        if !machine_trusted(subject) {
            log_line("signing certificate not trusted at machine level; one elevation required");
            let script = format!(
                "certutil -f -addstore TrustedPeople '{path}'; certutil -f -addstore Root '{path}'",
                path = path
            );
            run_elevated_now("powershell.exe", &format!("-NoLogo -NoProfile -Command \"{script}\""))?;
            // run_elevated waits for the elevated process to exit, so the
            // stores are settled — verify instead of hoping.
            if !machine_trusted(subject) {
                return Err(
                    "the signing certificate could not be trusted at machine level; \
                     Windows cannot register the app's package identity"
                        .to_string(),
                );
            }
            log_line("signing certificate now trusted at machine level");
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

/// Shell wrapper kept local so the elevated PowerShell failure text (which
/// certutil writes to stdout/stderr, invisible for a hidden window) still
/// lands in the log on failure.
fn run_elevated_now(program: &str, args: &str) -> Result<(), String> {
    let result = crate::util::run_elevated(program, args);
    if let Err(e) = &result {
        log_line(&format!("machine trust not established: {e}"));
    }
    result
}
