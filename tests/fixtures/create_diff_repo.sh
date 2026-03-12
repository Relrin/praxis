#!/bin/bash
# Creates a two-commit test repository for praxis diff integration tests.

set -euo pipefail

DIR="$1"
rm -rf "$DIR"
mkdir -p "$DIR"
cd "$DIR"
git init

# Commit 1: baseline
mkdir -p src

cat > src/auth.rs << 'EOF'
pub fn verify_token(token: &str) -> bool {
    token.len() > 10
}

pub fn check_session(session_id: &str) -> bool {
    !session_id.is_empty()
}
EOF

cat > src/middleware.rs << 'EOF'
use crate::auth;

pub fn auth_middleware(req: Request) -> Response {
    if auth::verify_token(&req.token) {
        handle(req)
    } else {
        Response::unauthorized()
    }
}
EOF

cat > src/handler.rs << 'EOF'
use crate::auth;

pub fn login_handler(req: Request) -> Response {
    let valid = auth::verify_token(&req.token);
    if valid {
        Response::ok()
    } else {
        Response::forbidden()
    }
}
EOF

git add -A
git commit -m "Initial commit"

# Commit 2: feature/auth changes
cat > src/auth.rs << 'EOF'
pub fn verify_token(token: &str, ctx: &AuthCtx) -> Result<Claims> {
    // Signature changed: added ctx parameter and Result return type
    ctx.validate(token)
}

pub fn refresh_token(token: &str) -> Result<String> {
    // New function
    Ok(format!("refreshed_{}", token))
}

// check_session removed
EOF

cat > src/middleware.rs << 'EOF'
use crate::auth;

pub async fn auth_middleware(req: Request) -> Result<Response> {
    // Signature changed: now async with Result
    let claims = auth::verify_token(&req.token, &req.ctx)?;
    Ok(handle(req, claims))
}
EOF

cat > src/token.rs << 'EOF'
// New file
pub struct Claims {
    pub sub: String,
    pub exp: u64,
}

pub struct AuthCtx {
    pub secret: String,
}

impl AuthCtx {
    pub fn validate(&self, token: &str) -> Result<Claims> {
        // ...
        Ok(Claims { sub: "user".into(), exp: 0 })
    }
}
EOF

# handler.rs unchanged

git add -A
git commit -m "Add OAuth2 auth with JWT tokens"
