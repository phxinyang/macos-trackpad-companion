# Security

The network listener can inject pointer and gesture events into the Mac. Use a
token (`[net].token`) for any network that is not fully trusted, and do not
expose the listener to the public internet. Pairing links contain the token and
must be treated as secrets.

Please report security issues privately before publishing reproduction details.
Do not include tokens, host names, full paths, or unredacted diagnostic logs in
issues or pull requests.
