## User
Let's add OAuth2 authentication to the API.

## Assistant
We should use the existing session module in src/middleware.rs as a base.

## User
We must not use external auth providers. Everything stays in-house.

## Assistant
Understood. Should we write our own token issuer?

## User
Yes, decided — use JWT tokens. The implementation goes in src/auth.rs.

## Assistant
Token expiry must be configurable. I'll add that to src/auth.rs.

## User
Going with BTreeMap for all output maps to keep things deterministic.

## Assistant
Agreed. Should we support refresh tokens?

## User
Not yet. Let's avoid adding complexity in this phase.

## Assistant
Confirmed. We are using src/token.rs for the token logic.
The src/middleware.rs file needs an async update too.
