//! Authentication and Role-Based Access Control (RBAC)
//!
//! Comprehensive security system for AetherShell:
//! - Token-based authentication (JWT, API keys)
//! - Role-based access control with permissions
//! - Session management
//! - Audit logging
//! - Multi-factor authentication support

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::value::Value;

// ============================================================================
// Core Types
// ============================================================================

/// User identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
    pub permissions: HashSet<String>,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub active: bool,
    pub mfa_enabled: bool,
}

impl User {
    pub fn new(username: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            username: username.into(),
            email: None,
            display_name: None,
            roles: Vec::new(),
            permissions: HashSet::new(),
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
            last_login: None,
            active: true,
            mfa_enabled: false,
        }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(&role.to_string())
    }
}

/// Role definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: HashSet<String>,
    pub parent_roles: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Role {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        let name = name.into();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.clone(),
            description: None,
            permissions: HashSet::new(),
            parent_roles: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permissions.insert(perm.into());
        self
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_roles.push(parent.into());
        self
    }
}

/// Permission definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub name: String,
    pub resource: String,
    pub action: String,
    pub description: Option<String>,
}

impl Permission {
    pub fn new(resource: impl Into<String>, action: impl Into<String>) -> Self {
        let resource = resource.into();
        let action = action.into();
        let name = format!("{}:{}", resource, action);
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            resource,
            action,
            description: None,
        }
    }
}

// ============================================================================
// Authentication Tokens
// ============================================================================

/// Token type enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenType {
    Bearer,
    ApiKey,
    Refresh,
    Service,
}

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub token_type: TokenType,
    pub user_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
    pub metadata: BTreeMap<String, String>,
    pub revoked: bool,
}

impl AuthToken {
    pub fn new(user_id: impl Into<String>, token_type: TokenType, ttl_minutes: i64) -> Self {
        let now = Utc::now();
        Self {
            token: generate_token(),
            token_type,
            user_id: user_id.into(),
            issued_at: now,
            expires_at: now + Duration::minutes(ttl_minutes),
            scopes: Vec::new(),
            metadata: BTreeMap::new(),
            revoked: false,
        }
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    pub fn is_valid(&self) -> bool {
        !self.revoked && Utc::now() < self.expires_at
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string()) || self.scopes.contains(&"*".to_string())
    }
}

fn generate_token() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// API Key for programmatic access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub user_id: String,
    pub scopes: Vec<String>,
    pub rate_limit: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used: Option<DateTime<Utc>>,
    pub revoked: bool,
}

impl ApiKey {
    pub fn new(name: impl Into<String>, user_id: impl Into<String>) -> (Self, String) {
        let raw_key = generate_token();
        let key_prefix = raw_key[..8].to_string();
        let key_hash = hash_key(&raw_key);

        let api_key = Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            key_hash,
            key_prefix,
            user_id: user_id.into(),
            scopes: Vec::new(),
            rate_limit: None,
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            revoked: false,
        };

        (api_key, raw_key)
    }

    pub fn verify(&self, key: &str) -> bool {
        !self.revoked && hash_key(key) == self.key_hash
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }
}

fn hash_key(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ============================================================================
// Session Management
// ============================================================================

/// User session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub refresh_token: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub active: bool,
}

impl Session {
    pub fn new(user_id: impl Into<String>, ttl_hours: i64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            token: generate_token(),
            refresh_token: Some(generate_token()),
            ip_address: None,
            user_agent: None,
            created_at: now,
            expires_at: now + Duration::hours(ttl_hours),
            last_activity: now,
            active: true,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.active && Utc::now() < self.expires_at
    }

    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }
}

// ============================================================================
// Audit Logging
// ============================================================================

/// Audit event type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    Login,
    Logout,
    LoginFailed,
    TokenCreated,
    TokenRevoked,
    PermissionGranted,
    PermissionDenied,
    RoleAssigned,
    RoleRemoved,
    UserCreated,
    UserUpdated,
    UserDeleted,
    ApiKeyCreated,
    ApiKeyRevoked,
    ConfigChanged,
    Custom(String),
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub user_id: Option<String>,
    pub resource: Option<String>,
    pub action: String,
    pub result: AuditResult,
    pub ip_address: Option<String>,
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure(String),
}

impl AuditEntry {
    pub fn new(event_type: AuditEventType, action: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            user_id: None,
            resource: None,
            action: action.into(),
            result: AuditResult::Success,
            ip_address: None,
            details: BTreeMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    pub fn with_result(mut self, result: AuditResult) -> Self {
        self.result = result;
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// RBAC Manager
// ============================================================================

/// Role-Based Access Control Manager
pub struct RbacManager {
    users: RwLock<HashMap<String, User>>,
    roles: RwLock<HashMap<String, Role>>,
    permissions: RwLock<HashMap<String, Permission>>,
    // Cache resolved permissions for users
    permission_cache: RwLock<HashMap<String, HashSet<String>>>,
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RbacManager {
    pub fn new() -> Self {
        let manager = Self {
            users: RwLock::new(HashMap::new()),
            roles: RwLock::new(HashMap::new()),
            permissions: RwLock::new(HashMap::new()),
            permission_cache: RwLock::new(HashMap::new()),
        };
        manager.setup_default_roles();
        manager
    }

    fn setup_default_roles(&self) {
        // Admin role with all permissions
        let admin = Role::new("admin")
            .with_description("Full system access")
            .with_permission("*:*");
        self.add_role(admin);

        // Operator role
        let operator = Role::new("operator")
            .with_description("Operations access")
            .with_permission("agent:read")
            .with_permission("agent:execute")
            .with_permission("workflow:read")
            .with_permission("workflow:execute")
            .with_permission("metrics:read");
        self.add_role(operator);

        // Viewer role
        let viewer = Role::new("viewer")
            .with_description("Read-only access")
            .with_permission("agent:read")
            .with_permission("workflow:read")
            .with_permission("metrics:read");
        self.add_role(viewer);

        // Agent role for automated agents
        let agent_role = Role::new("agent")
            .with_description("Agent service account")
            .with_permission("agent:execute")
            .with_permission("tool:use")
            .with_permission("api:call");
        self.add_role(agent_role);
    }

    pub fn add_user(&self, user: User) -> Result<()> {
        let mut users = self.users.write().map_err(|_| anyhow!("Lock poisoned"))?;
        users.insert(user.id.clone(), user);
        Ok(())
    }

    pub fn get_user(&self, user_id: &str) -> Option<User> {
        self.users.read().ok()?.get(user_id).cloned()
    }

    pub fn get_user_by_username(&self, username: &str) -> Option<User> {
        self.users
            .read()
            .ok()?
            .values()
            .find(|u| u.username == username)
            .cloned()
    }

    pub fn add_role(&self, role: Role) {
        if let Ok(mut roles) = self.roles.write() {
            roles.insert(role.name.clone(), role);
        }
    }

    pub fn get_role(&self, name: &str) -> Option<Role> {
        self.roles.read().ok()?.get(name).cloned()
    }

    pub fn assign_role(&self, user_id: &str, role_name: &str) -> Result<()> {
        let mut users = self.users.write().map_err(|_| anyhow!("Lock poisoned"))?;
        let user = users
            .get_mut(user_id)
            .ok_or_else(|| anyhow!("User not found"))?;

        if !user.roles.contains(&role_name.to_string()) {
            user.roles.push(role_name.to_string());
            user.updated_at = Utc::now();
        }

        // Invalidate permission cache
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.remove(user_id);
        }

        Ok(())
    }

    pub fn remove_role(&self, user_id: &str, role_name: &str) -> Result<()> {
        let mut users = self.users.write().map_err(|_| anyhow!("Lock poisoned"))?;
        let user = users
            .get_mut(user_id)
            .ok_or_else(|| anyhow!("User not found"))?;

        user.roles.retain(|r| r != role_name);
        user.updated_at = Utc::now();

        // Invalidate permission cache
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.remove(user_id);
        }

        Ok(())
    }

    pub fn resolve_permissions(&self, user_id: &str) -> HashSet<String> {
        // Check cache first
        if let Ok(cache) = self.permission_cache.read() {
            if let Some(perms) = cache.get(user_id) {
                return perms.clone();
            }
        }

        let user = match self.get_user(user_id) {
            Some(u) => u,
            None => return HashSet::new(),
        };

        let mut permissions = user.permissions.clone();

        // Resolve role permissions
        let roles = self.roles.read().ok();
        if let Some(roles) = roles {
            let mut visited = HashSet::new();
            for role_name in &user.roles {
                self.resolve_role_permissions(role_name, &roles, &mut permissions, &mut visited);
            }
        }

        // Update cache
        if let Ok(mut cache) = self.permission_cache.write() {
            cache.insert(user_id.to_string(), permissions.clone());
        }

        permissions
    }

    fn resolve_role_permissions(
        &self,
        role_name: &str,
        roles: &HashMap<String, Role>,
        permissions: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        if visited.contains(role_name) {
            return; // Avoid cycles
        }
        visited.insert(role_name.to_string());

        if let Some(role) = roles.get(role_name) {
            permissions.extend(role.permissions.clone());

            // Resolve parent roles recursively
            for parent in &role.parent_roles {
                self.resolve_role_permissions(parent, roles, permissions, visited);
            }
        }
    }

    pub fn check_permission(&self, user_id: &str, permission: &str) -> bool {
        let permissions = self.resolve_permissions(user_id);

        // Check wildcard permissions
        if permissions.contains("*:*") {
            return true;
        }

        // Check resource wildcard
        let parts: Vec<&str> = permission.split(':').collect();
        if parts.len() == 2 {
            let resource_wildcard = format!("{}:*", parts[0]);
            if permissions.contains(&resource_wildcard) {
                return true;
            }
        }

        permissions.contains(permission)
    }

    pub fn list_users(&self) -> Vec<User> {
        self.users
            .read()
            .map(|u| u.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn list_roles(&self) -> Vec<Role> {
        self.roles
            .read()
            .map(|r| r.values().cloned().collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// Authentication Manager
// ============================================================================

/// Credential type for authentication
#[derive(Debug, Clone)]
pub enum Credential {
    Password { username: String, password: String },
    Token { token: String },
    ApiKey { key: String },
    OAuth { provider: String, code: String },
}

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub success: bool,
    pub user: Option<User>,
    pub session: Option<Session>,
    pub token: Option<AuthToken>,
    pub error: Option<String>,
}

impl AuthResult {
    pub fn success(user: User, session: Session, token: AuthToken) -> Self {
        Self {
            success: true,
            user: Some(user),
            session: Some(session),
            token: Some(token),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            user: None,
            session: None,
            token: None,
            error: Some(error.into()),
        }
    }
}

/// Authentication Manager
pub struct AuthManager {
    rbac: Arc<RbacManager>,
    sessions: RwLock<HashMap<String, Session>>,
    tokens: RwLock<HashMap<String, AuthToken>>,
    api_keys: RwLock<HashMap<String, ApiKey>>,
    audit_log: RwLock<Vec<AuditEntry>>,
    password_hashes: RwLock<HashMap<String, String>>,
    max_audit_entries: usize,
    session_ttl_hours: i64,
    token_ttl_minutes: i64,
}

impl AuthManager {
    pub fn new(rbac: Arc<RbacManager>) -> Self {
        Self {
            rbac,
            sessions: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
            api_keys: RwLock::new(HashMap::new()),
            audit_log: RwLock::new(Vec::new()),
            password_hashes: RwLock::new(HashMap::new()),
            max_audit_entries: 10000,
            session_ttl_hours: 24,
            token_ttl_minutes: 60,
        }
    }

    /// Register a new user with password
    pub fn register_user(&self, username: &str, password: &str) -> Result<User> {
        // Check if username exists
        if self.rbac.get_user_by_username(username).is_some() {
            return Err(anyhow!("Username already exists"));
        }

        let user = User::new(username);
        let password_hash = hash_key(password);

        // Store user
        self.rbac.add_user(user.clone())?;

        // Store password hash
        if let Ok(mut hashes) = self.password_hashes.write() {
            hashes.insert(user.id.clone(), password_hash);
        }

        // Audit
        self.audit(
            AuditEntry::new(AuditEventType::UserCreated, "register")
                .with_user(&user.id)
                .with_detail("username", username),
        );

        Ok(user)
    }

    /// Authenticate with credentials
    pub fn authenticate(&self, credential: Credential) -> AuthResult {
        match credential {
            Credential::Password { username, password } => {
                self.authenticate_password(&username, &password)
            }
            Credential::Token { token } => self.authenticate_token(&token),
            Credential::ApiKey { key } => self.authenticate_api_key(&key),
            Credential::OAuth { provider, code } => self.authenticate_oauth(&provider, &code),
        }
    }

    fn authenticate_password(&self, username: &str, password: &str) -> AuthResult {
        let user = match self.rbac.get_user_by_username(username) {
            Some(u) => u,
            None => {
                self.audit(
                    AuditEntry::new(AuditEventType::LoginFailed, "login")
                        .with_detail("username", username)
                        .with_result(AuditResult::Failure("User not found".to_string())),
                );
                return AuthResult::failure("Invalid credentials");
            }
        };

        if !user.active {
            self.audit(
                AuditEntry::new(AuditEventType::LoginFailed, "login")
                    .with_user(&user.id)
                    .with_result(AuditResult::Failure("User inactive".to_string())),
            );
            return AuthResult::failure("User account is inactive");
        }

        // Verify password
        let password_hash = hash_key(password);
        let stored_hash = self
            .password_hashes
            .read()
            .ok()
            .and_then(|h| h.get(&user.id).cloned());

        if stored_hash != Some(password_hash) {
            self.audit(
                AuditEntry::new(AuditEventType::LoginFailed, "login")
                    .with_user(&user.id)
                    .with_result(AuditResult::Failure("Invalid password".to_string())),
            );
            return AuthResult::failure("Invalid credentials");
        }

        // Create session and token
        let session = Session::new(&user.id, self.session_ttl_hours);
        let token = AuthToken::new(&user.id, TokenType::Bearer, self.token_ttl_minutes);

        // Store session and token
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.insert(session.id.clone(), session.clone());
        }
        if let Ok(mut tokens) = self.tokens.write() {
            tokens.insert(token.token.clone(), token.clone());
        }

        // Update last login
        if let Ok(mut users) = self.rbac.users.write() {
            if let Some(u) = users.get_mut(&user.id) {
                u.last_login = Some(Utc::now());
            }
        }

        self.audit(
            AuditEntry::new(AuditEventType::Login, "login")
                .with_user(&user.id)
                .with_detail("session_id", &session.id),
        );

        AuthResult::success(user, session, token)
    }

    fn authenticate_token(&self, token: &str) -> AuthResult {
        let auth_token = match self.tokens.read().ok().and_then(|t| t.get(token).cloned()) {
            Some(t) => t,
            None => return AuthResult::failure("Invalid token"),
        };

        if !auth_token.is_valid() {
            return AuthResult::failure("Token expired or revoked");
        }

        let user = match self.rbac.get_user(&auth_token.user_id) {
            Some(u) => u,
            None => return AuthResult::failure("User not found"),
        };

        if !user.active {
            return AuthResult::failure("User account is inactive");
        }

        // Create new session for token auth
        let session = Session::new(&user.id, self.session_ttl_hours);
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.insert(session.id.clone(), session.clone());
        }

        AuthResult::success(user, session, auth_token)
    }

    fn authenticate_api_key(&self, key: &str) -> AuthResult {
        let api_key = self
            .api_keys
            .read()
            .ok()
            .and_then(|keys| keys.values().find(|k| k.verify(key)).cloned());

        let api_key = match api_key {
            Some(k) => k,
            None => return AuthResult::failure("Invalid API key"),
        };

        if api_key.revoked || api_key.is_expired() {
            return AuthResult::failure("API key is revoked or expired");
        }

        let user = match self.rbac.get_user(&api_key.user_id) {
            Some(u) => u,
            None => return AuthResult::failure("User not found"),
        };

        // Update last used
        if let Ok(mut keys) = self.api_keys.write() {
            if let Some(k) = keys.get_mut(&api_key.id) {
                k.last_used = Some(Utc::now());
            }
        }

        let session = Session::new(&user.id, self.session_ttl_hours);
        let token = AuthToken::new(&user.id, TokenType::ApiKey, self.token_ttl_minutes);

        AuthResult::success(user, session, token)
    }

    fn authenticate_oauth(&self, _provider: &str, _code: &str) -> AuthResult {
        // OAuth implementation would go here
        AuthResult::failure("OAuth not implemented")
    }

    /// Create a new API key
    pub fn create_api_key(&self, user_id: &str, name: &str) -> Result<(ApiKey, String)> {
        let user = self
            .rbac
            .get_user(user_id)
            .ok_or_else(|| anyhow!("User not found"))?;

        let (api_key, raw_key) = ApiKey::new(name, &user.id);

        if let Ok(mut keys) = self.api_keys.write() {
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        self.audit(
            AuditEntry::new(AuditEventType::ApiKeyCreated, "create_api_key")
                .with_user(user_id)
                .with_detail("key_id", &api_key.id)
                .with_detail("key_name", name),
        );

        Ok((api_key, raw_key))
    }

    /// Revoke an API key
    pub fn revoke_api_key(&self, key_id: &str) -> Result<()> {
        if let Ok(mut keys) = self.api_keys.write() {
            if let Some(key) = keys.get_mut(key_id) {
                key.revoked = true;

                self.audit(
                    AuditEntry::new(AuditEventType::ApiKeyRevoked, "revoke_api_key")
                        .with_user(&key.user_id)
                        .with_detail("key_id", key_id),
                );

                return Ok(());
            }
        }
        Err(anyhow!("API key not found"))
    }

    /// Validate a session
    pub fn validate_session(&self, session_id: &str) -> Option<Session> {
        let session = self.sessions.read().ok()?.get(session_id).cloned()?;

        if session.is_valid() {
            // Touch session
            if let Ok(mut sessions) = self.sessions.write() {
                if let Some(s) = sessions.get_mut(session_id) {
                    s.touch();
                }
            }
            Some(session)
        } else {
            None
        }
    }

    /// Logout / invalidate session
    pub fn logout(&self, session_id: &str) -> Result<()> {
        if let Ok(mut sessions) = self.sessions.write() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.active = false;

                self.audit(
                    AuditEntry::new(AuditEventType::Logout, "logout")
                        .with_user(&session.user_id)
                        .with_detail("session_id", session_id),
                );

                return Ok(());
            }
        }
        Err(anyhow!("Session not found"))
    }

    /// Check if user has permission
    pub fn check_permission(&self, user_id: &str, permission: &str) -> bool {
        let allowed = self.rbac.check_permission(user_id, permission);

        if !allowed {
            self.audit(
                AuditEntry::new(AuditEventType::PermissionDenied, "check_permission")
                    .with_user(user_id)
                    .with_resource(permission)
                    .with_result(AuditResult::Failure("Permission denied".to_string())),
            );
        }

        allowed
    }

    /// Get audit log
    pub fn get_audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        self.audit_log
            .read()
            .map(|l| l.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    /// Get audit log for a specific user
    pub fn get_user_audit_log(&self, user_id: &str, limit: usize) -> Vec<AuditEntry> {
        self.audit_log
            .read()
            .map(|l| {
                l.iter()
                    .filter(|e| e.user_id.as_ref() == Some(&user_id.to_string()))
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn audit(&self, entry: AuditEntry) {
        if let Ok(mut log) = self.audit_log.write() {
            if log.len() >= self.max_audit_entries {
                log.remove(0);
            }
            log.push(entry);
        }
    }

    /// Get active sessions for a user
    pub fn get_user_sessions(&self, user_id: &str) -> Vec<Session> {
        self.sessions
            .read()
            .map(|s| {
                s.values()
                    .filter(|session| session.user_id == user_id && session.is_valid())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Export auth summary as Value
    pub fn summary(&self) -> Value {
        let mut result: BTreeMap<String, Value> = BTreeMap::new();

        // Users
        let users: Vec<Value> = self
            .rbac
            .list_users()
            .iter()
            .map(|u| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("id".to_string(), Value::Str(u.id.clone()));
                m.insert("username".to_string(), Value::Str(u.username.clone()));
                m.insert(
                    "roles".to_string(),
                    Value::Array(u.roles.iter().map(|r| Value::Str(r.clone())).collect()),
                );
                m.insert("active".to_string(), Value::Bool(u.active));
                Value::Record(m)
            })
            .collect();
        result.insert("users".to_string(), Value::Array(users));

        // Roles
        let roles: Vec<Value> = self
            .rbac
            .list_roles()
            .iter()
            .map(|r| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert("name".to_string(), Value::Str(r.name.clone()));
                m.insert(
                    "permissions".to_string(),
                    Value::Array(
                        r.permissions
                            .iter()
                            .map(|p| Value::Str(p.clone()))
                            .collect(),
                    ),
                );
                Value::Record(m)
            })
            .collect();
        result.insert("roles".to_string(), Value::Array(roles));

        // Active sessions count
        let active_sessions = self
            .sessions
            .read()
            .map(|s| s.values().filter(|sess| sess.is_valid()).count())
            .unwrap_or(0);
        result.insert(
            "active_sessions".to_string(),
            Value::Int(active_sessions as i64),
        );

        // API keys count
        let api_keys = self
            .api_keys
            .read()
            .map(|k| k.values().filter(|key| !key.revoked).count())
            .unwrap_or(0);
        result.insert("active_api_keys".to_string(), Value::Int(api_keys as i64));

        // Recent audit entries
        let audit: Vec<Value> = self
            .get_audit_log(10)
            .iter()
            .map(|e| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                m.insert(
                    "timestamp".to_string(),
                    Value::Str(e.timestamp.to_rfc3339()),
                );
                m.insert(
                    "event".to_string(),
                    Value::Str(format!("{:?}", e.event_type)),
                );
                m.insert("action".to_string(), Value::Str(e.action.clone()));
                if let Some(ref user) = e.user_id {
                    m.insert("user".to_string(), Value::Str(user.clone()));
                }
                Value::Record(m)
            })
            .collect();
        result.insert("recent_audit".to_string(), Value::Array(audit));

        Value::Record(result)
    }
}

// ============================================================================
// Builtins for Shell Integration
// ============================================================================

/// Create auth builtins for AetherShell
pub fn create_auth_builtins() -> BTreeMap<String, Value> {
    let mut builtins: BTreeMap<String, Value> = BTreeMap::new();

    builtins.insert(
        "auth_login".to_string(),
        Value::Str("Authenticate user".to_string()),
    );
    builtins.insert(
        "auth_logout".to_string(),
        Value::Str("End current session".to_string()),
    );
    builtins.insert(
        "auth_whoami".to_string(),
        Value::Str("Get current user info".to_string()),
    );
    builtins.insert(
        "auth_create_key".to_string(),
        Value::Str("Create API key".to_string()),
    );
    builtins.insert(
        "auth_revoke_key".to_string(),
        Value::Str("Revoke API key".to_string()),
    );
    builtins.insert(
        "auth_check".to_string(),
        Value::Str("Check permission".to_string()),
    );
    builtins.insert(
        "auth_audit".to_string(),
        Value::Str("View audit log".to_string()),
    );
    builtins.insert(
        "rbac_users".to_string(),
        Value::Str("List users".to_string()),
    );
    builtins.insert(
        "rbac_roles".to_string(),
        Value::Str("List roles".to_string()),
    );
    builtins.insert(
        "rbac_assign".to_string(),
        Value::Str("Assign role to user".to_string()),
    );

    builtins
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new("testuser")
            .with_email("test@example.com")
            .with_role("viewer");

        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, Some("test@example.com".to_string()));
        assert!(user.has_role("viewer"));
        assert!(user.active);
    }

    #[test]
    fn test_role_creation() {
        let role = Role::new("custom")
            .with_description("Custom role")
            .with_permission("resource:read")
            .with_permission("resource:write");

        assert_eq!(role.name, "custom");
        assert!(role.permissions.contains("resource:read"));
        assert!(role.permissions.contains("resource:write"));
    }

    #[test]
    fn test_permission_format() {
        let perm = Permission::new("agent", "execute");
        assert_eq!(perm.name, "agent:execute");
        assert_eq!(perm.resource, "agent");
        assert_eq!(perm.action, "execute");
    }

    #[test]
    fn test_auth_token() {
        let token = AuthToken::new("user123", TokenType::Bearer, 60)
            .with_scope("read")
            .with_scope("write");

        assert!(token.is_valid());
        assert!(token.has_scope("read"));
        assert!(token.has_scope("write"));
        assert!(!token.has_scope("admin"));
    }

    #[test]
    fn test_api_key() {
        let (api_key, raw_key) = ApiKey::new("test-key", "user123");

        assert!(api_key.verify(&raw_key));
        assert!(!api_key.verify("wrong-key"));
        assert!(!api_key.revoked);
    }

    #[test]
    fn test_session() {
        let session = Session::new("user123", 24);

        assert!(session.is_valid());
        assert_eq!(session.user_id, "user123");
        assert!(session.active);
    }

    #[test]
    fn test_rbac_manager_default_roles() {
        let rbac = RbacManager::new();

        assert!(rbac.get_role("admin").is_some());
        assert!(rbac.get_role("operator").is_some());
        assert!(rbac.get_role("viewer").is_some());
    }

    #[test]
    fn test_rbac_permission_resolution() {
        let rbac = RbacManager::new();

        let user = User::new("testuser");
        rbac.add_user(user.clone()).unwrap();
        rbac.assign_role(&user.id, "viewer").unwrap();

        assert!(rbac.check_permission(&user.id, "agent:read"));
        assert!(!rbac.check_permission(&user.id, "agent:write"));
    }

    #[test]
    fn test_rbac_admin_wildcard() {
        let rbac = RbacManager::new();

        let user = User::new("admin_user");
        rbac.add_user(user.clone()).unwrap();
        rbac.assign_role(&user.id, "admin").unwrap();

        // Admin should have all permissions
        assert!(rbac.check_permission(&user.id, "anything:everything"));
    }

    #[test]
    fn test_auth_manager_registration() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(rbac);

        let user = auth.register_user("newuser", "password123").unwrap();
        assert_eq!(user.username, "newuser");
    }

    #[test]
    fn test_auth_manager_login() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("logintest", "secret").unwrap();

        let result = auth.authenticate(Credential::Password {
            username: "logintest".to_string(),
            password: "secret".to_string(),
        });

        assert!(result.success);
        assert!(result.user.is_some());
        assert!(result.session.is_some());
        assert!(result.token.is_some());
    }

    #[test]
    fn test_auth_manager_invalid_login() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("user1", "correct").unwrap();

        let result = auth.authenticate(Credential::Password {
            username: "user1".to_string(),
            password: "wrong".to_string(),
        });

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_api_key_authentication() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        let user = auth.register_user("apiuser", "pass").unwrap();
        let (_, raw_key) = auth.create_api_key(&user.id, "test-key").unwrap();

        let result = auth.authenticate(Credential::ApiKey { key: raw_key });

        assert!(result.success);
    }

    #[test]
    fn test_token_authentication() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("tokenuser", "pass").unwrap();

        let login_result = auth.authenticate(Credential::Password {
            username: "tokenuser".to_string(),
            password: "pass".to_string(),
        });

        let token = login_result.token.unwrap().token;

        let result = auth.authenticate(Credential::Token { token });
        assert!(result.success);
    }

    #[test]
    fn test_session_validation() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("sessuser", "pass").unwrap();

        let result = auth.authenticate(Credential::Password {
            username: "sessuser".to_string(),
            password: "pass".to_string(),
        });

        let session_id = result.session.unwrap().id;

        assert!(auth.validate_session(&session_id).is_some());
        assert!(auth.validate_session("invalid").is_none());
    }

    #[test]
    fn test_logout() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("logoutuser", "pass").unwrap();

        let result = auth.authenticate(Credential::Password {
            username: "logoutuser".to_string(),
            password: "pass".to_string(),
        });

        let session_id = result.session.unwrap().id;

        auth.logout(&session_id).unwrap();

        // Session should no longer be valid
        assert!(auth.validate_session(&session_id).is_none());
    }

    #[test]
    fn test_audit_log() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("audituser", "pass").unwrap();

        auth.authenticate(Credential::Password {
            username: "audituser".to_string(),
            password: "pass".to_string(),
        });

        let audit = auth.get_audit_log(10);
        assert!(!audit.is_empty());

        let has_login = audit.iter().any(|e| e.event_type == AuditEventType::Login);
        assert!(has_login);
    }

    #[test]
    fn test_revoke_api_key() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        let user = auth.register_user("revokeuser", "pass").unwrap();
        let (api_key, raw_key) = auth.create_api_key(&user.id, "revoke-test").unwrap();

        // Should work before revocation
        let result = auth.authenticate(Credential::ApiKey {
            key: raw_key.clone(),
        });
        assert!(result.success);

        // Revoke
        auth.revoke_api_key(&api_key.id).unwrap();

        // Should fail after revocation
        let result = auth.authenticate(Credential::ApiKey { key: raw_key });
        assert!(!result.success);
    }

    #[test]
    fn test_permission_check_with_audit() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        let user = auth.register_user("permuser", "pass").unwrap();
        rbac.assign_role(&user.id, "viewer").unwrap();

        // This should pass
        assert!(auth.check_permission(&user.id, "agent:read"));

        // This should fail and be audited
        assert!(!auth.check_permission(&user.id, "admin:action"));

        let audit = auth.get_audit_log(10);
        let has_denied = audit
            .iter()
            .any(|e| e.event_type == AuditEventType::PermissionDenied);
        assert!(has_denied);
    }

    #[test]
    fn test_auth_summary() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("summaryuser", "pass").unwrap();

        let summary = auth.summary();
        match summary {
            Value::Record(r) => {
                assert!(r.contains_key("users"));
                assert!(r.contains_key("roles"));
                assert!(r.contains_key("active_sessions"));
            }
            _ => panic!("Expected Record"),
        }
    }

    #[test]
    fn test_audit_entry_builder() {
        let entry = AuditEntry::new(AuditEventType::Login, "test_action")
            .with_user("user123")
            .with_resource("session")
            .with_detail("ip", "127.0.0.1")
            .with_result(AuditResult::Success);

        assert_eq!(entry.event_type, AuditEventType::Login);
        assert_eq!(entry.user_id, Some("user123".to_string()));
        assert_eq!(entry.resource, Some("session".to_string()));
        assert_eq!(entry.details.get("ip"), Some(&"127.0.0.1".to_string()));
    }

    #[test]
    fn test_role_inheritance() {
        let rbac = RbacManager::new();

        // Create a role that inherits from viewer
        let custom = Role::new("custom_viewer")
            .with_parent("viewer")
            .with_permission("custom:action");
        rbac.add_role(custom);

        let user = User::new("inherituser");
        rbac.add_user(user.clone()).unwrap();
        rbac.assign_role(&user.id, "custom_viewer").unwrap();

        // Should have viewer permissions
        assert!(rbac.check_permission(&user.id, "agent:read"));
        // Should have custom permission
        assert!(rbac.check_permission(&user.id, "custom:action"));
    }

    #[test]
    fn test_auth_builtins() {
        let builtins = create_auth_builtins();
        assert!(builtins.contains_key("auth_login"));
        assert!(builtins.contains_key("auth_logout"));
        assert!(builtins.contains_key("rbac_users"));
        assert!(builtins.contains_key("rbac_assign"));
    }

    #[test]
    fn test_duplicate_username() {
        let rbac = Arc::new(RbacManager::new());
        let auth = AuthManager::new(Arc::clone(&rbac));

        auth.register_user("duplicate", "pass1").unwrap();
        let result = auth.register_user("duplicate", "pass2");

        assert!(result.is_err());
    }
}
