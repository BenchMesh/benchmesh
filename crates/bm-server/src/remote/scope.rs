//! What a route lets its caller do. A remote session holds every scope except `Admin`; a route
//! that is not classified here is `Admin`, so a new route is local-only until it is listed.

use axum::http::Method;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Read and control instruments, the UI itself, API description.
    Instruments,
    /// The instrument configuration (add, change, remove, detect).
    Configuration,
    Recordings,
    Automation,
    /// The computer: resources, restart / shut down and start with the computer of a headless
    /// server (headless, everything is done remotely).
    System,
    /// Remote access itself, MCP, assistant keys, the Windows driver installs: this computer only.
    Admin,
}

pub fn of(method: &Method, path: &str) -> Scope {
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match segs.as_slice() {
        [""] | ["status"] | ["version"] | ["catalog"] | ["docs"] | ["redoc"] | ["openapi.json"] | ["openapi.yaml"] => Scope::Instruments,
        ["ui", ..] | ["instruments", ..] | ["drivers", ..] | ["metrics", ..] => Scope::Instruments,
        ["ws", "registry"] | ["ws", "metrics"] | ["environment"] | ["ai", "context"] | ["ai", "system-prompt"] => Scope::Instruments,
        ["remote", "session"] | ["remote", "logout"] | ["remote", "login"] => Scope::Instruments,
        ["devices", _, "wire"] | ["devices", _, "park"] => Scope::Instruments,
        ["config"] | ["devices", ..] | ["serial-ports"] => Scope::Configuration,
        ["usbtmc-devices"] if method == Method::GET => Scope::Configuration,
        ["recordings", ..] | ["ws", "recordings"] => Scope::Recordings,
        ["system", ..] => Scope::System,
        // a driver install asks for administrator approval on this computer
        ["sensors", .., "driver"] => Scope::Admin,
        // settings of sensors a distribution adds (the official build: the environmental sensor)
        ["sensors", ..] => Scope::Configuration,
        ["automation", "mcp"] => Scope::Admin,
        ["automation", "assistant"] if method != Method::GET => Scope::Admin,
        ["automation", ..] => Scope::Automation,
        _ => Scope::Admin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(m: &str, p: &str) -> Scope {
        of(&Method::from_bytes(m.as_bytes()).unwrap(), p)
    }

    #[test]
    fn routes_have_the_expected_scopes() {
        assert_eq!(s("POST", "/instruments/PSU/psu-1/1/output/ON"), Scope::Instruments);
        assert_eq!(s("GET", "/ui/assets/index.js"), Scope::Instruments);
        assert_eq!(s("GET", "/"), Scope::Instruments);
        assert_eq!(s("GET", "/devices/psu-1/wire"), Scope::Instruments);
        assert_eq!(s("POST", "/devices/psu-1/park"), Scope::Instruments, "parking is using the bench, not configuring it");
        assert_eq!(s("DELETE", "/devices/psu-1/park"), Scope::Instruments);
        assert_eq!(s("DELETE", "/devices/psu-1"), Scope::Configuration);
        assert_eq!(s("POST", "/devices/detect"), Scope::Configuration);
        assert_eq!(s("GET", "/usbtmc-devices"), Scope::Configuration);
        assert_eq!(s("POST", "/usbtmc-devices/driver"), Scope::Admin);
        assert_eq!(s("GET", "/ws/recordings"), Scope::Recordings);
        assert_eq!(s("GET", "/environment"), Scope::Instruments, "the lab's readings next to the instruments'");
        assert_eq!(s("POST", "/automation/run"), Scope::Automation);
        assert_eq!(s("GET", "/automation/assistant"), Scope::Automation);
        assert_eq!(s("PUT", "/automation/assistant"), Scope::Admin);
        assert_eq!(s("GET", "/automation/mcp"), Scope::Admin);
        assert_eq!(s("POST", "/mcp"), Scope::Admin);
        assert_eq!(s("PUT", "/remote"), Scope::Admin);
        assert_eq!(s("PUT", "/remote/password"), Scope::Admin);
        assert_eq!(s("PUT", "/remote/view-password"), Scope::Admin);
        assert_eq!(s("DELETE", "/remote/sessions"), Scope::Admin);
        assert_eq!(s("GET", "/remote/session"), Scope::Instruments);
        assert_eq!(s("PUT", "/scpi"), Scope::Admin, "virtual SCPI ports are set up on this computer only");
        assert_eq!(s("GET", "/system"), Scope::System);
        assert_eq!(s("POST", "/system/restart"), Scope::System, "headless, the browser is the only way to restart");
        assert_eq!(s("PUT", "/system/autostart"), Scope::System);
        assert_eq!(s("PUT", "/sensors/environmental"), Scope::Configuration);
        assert_eq!(s("POST", "/sensors/environmental/driver"), Scope::Admin, "the Windows driver install of a sensor adapter");
        assert_eq!(s("GET", "/something-new"), Scope::Admin, "unknown routes are local-only");
    }

    /// Every route of the router, and which ones are local-only: a new route must be placed
    /// in a scope on purpose.
    #[test]
    fn only_the_intended_routes_are_local_only() {
        let admin: Vec<String> = crate::openapi::ROUTES
            .iter()
            .filter(|(m, p, _, _)| of(&Method::from_bytes(m.to_uppercase().as_bytes()).unwrap(), &p.replace(['{', '}'], "")) == Scope::Admin)
            .map(|(m, p, _, _)| format!("{m} {p}"))
            .collect();
        assert_eq!(
            admin,
            [
                "post /usbtmc-devices/driver",
                "get /scpi",
                "put /scpi",
                "put /automation/assistant",
                "get /automation/mcp",
                "put /automation/mcp",
                "get /mcp",
                "post /mcp",
                "get /remote",
                "put /remote",
                "put /remote/password",
                "put /remote/view-password",
                "delete /remote/view-password",
                "delete /remote/sessions",
                "delete /remote/sessions/{id}",
            ]
        );
    }
}
