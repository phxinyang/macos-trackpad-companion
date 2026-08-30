# Security

The network listener can inject pointer and gesture events into the Mac. Use a
token (`[net].token`) for any network that is not fully trusted, and do not
expose the listener to the public internet. Pairing links contain the token and
must be treated as secrets.

Please report security issues privately before publishing reproduction details.
Open the repository's **Security** tab and use **Report a vulnerability** when
that option is available. If private vulnerability reporting is unavailable,
use a non-public contact method listed on a maintainer's GitHub profile and ask
for a private reporting channel. If neither route is available, open a minimal
public issue asking the maintainers to enable private reporting, without any
vulnerability details.

Do not include tokens, host names, full paths, or unredacted diagnostic logs in
issues or pull requests.
