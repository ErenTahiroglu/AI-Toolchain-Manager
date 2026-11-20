//! The `rmcp` handler. All lifecycle, era negotiation, `server/discover` and dispatch logic stays inside rmcp; this file only lists and
//! calls the four tools and restricts the supported protocol versions.

use crate::tools::{self, ToolError};
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ErrorData as McpError, Implementation, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use std::borrow::Cow;
use std::sync::Arc;

static SUPPORTED: [ProtocolVersion; 3] = [ProtocolVersion::V_2025_06_18, ProtocolVersion::V_2025_11_25, ProtocolVersion::V_2026_07_28];

#[derive(Clone, Default)]
pub struct AtmMcp;

fn tool_list() -> Vec<Tool> {
    tools::definitions()
        .into_iter()
        .map(|(name, description, input, output)| {
            let mut tool = Tool::new(name, description, Arc::new(input));
            tool.output_schema = Some(Arc::new(output));
            // Hints are advisory for hosts; they are NOT an operating-system protection.
            tool.annotations = Some(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false));
            tool
        })
        .collect()
}

fn tool_error(e: ToolError) -> CallToolResult {
    CallToolResult::structured_error(tools::error_value(e))
}

impl ServerHandler for AtmMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(tools::SERVER_NAME, tools::SERVER_VERSION))
            .with_instructions(tools::INSTRUCTIONS)
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&SUPPORTED)
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let mut result = ListToolsResult::default().with_ttl_ms(3_600_000).with_cache_scope(CacheScope::Private);
        result.tools = tool_list();
        Ok(result)
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        if request.input_responses.is_some() || request.request_state.is_some() {
            return Err(McpError::invalid_params("Invalid parameters", None));
        }
        let empty = serde_json::Map::new();
        let args = request.arguments.as_ref().unwrap_or(&empty);
        match tools::dispatch(&request.name, args) {
            None => Err(McpError::invalid_params("Unknown tool", None)),
            Some(Ok((value, _cap))) => Ok(CallToolResult::structured(value).into()),
            Some(Err(e)) => Ok(tool_error(e).into()),
        }
    }
}
