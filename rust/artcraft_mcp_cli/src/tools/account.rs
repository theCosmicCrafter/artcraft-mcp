use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use artcraft_api_defs::stripe_artcraft::create_credits_pack_checkout::StripeArtcraftCreateCreditsPackCheckoutRequest;
use artcraft_api_defs::stripe_artcraft::create_subscription_checkout::{
    PlanBillingCadence, StripeArtcraftCreateSubscriptionCheckoutRequest,
};
use artcraft_api_defs::stripe_artcraft::customer_portal_cancel_plan::StripeArtcraftCustomerPortalCancelPlanRequest;
use artcraft_api_defs::stripe_artcraft::customer_portal_manage_plan::StripeArtcraftCustomerPortalManagePlanRequest;
use artcraft_api_defs::stripe_artcraft::customer_portal_switch_plan::{
    PlanBillingCadenceConfirmation, StripeArtcraftCustomerPortalSwitchPlanRequest,
};
use artcraft_api_defs::stripe_artcraft::customer_portal_update_payment_method::StripeArtcraftCustomerPortalUpdatePaymentMethodRequest;
use enums::common::artcraft_credits_pack_slug::ArtcraftCreditsPackSlug;
use enums::common::artcraft_subscription_slug::ArtcraftSubscriptionSlug;

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_get, raw_json_post, typed_json_post};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_get_session_info".to_string(),
            description: "Get the currently authenticated user's info.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {}
            })),
        },
        Tool {
            name: "artcraft_get_credits".to_string(),
            description: "Get wallet credits and usage info.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "enum": ["artcraft", "fakeyou"], "description": "Payment namespace to query credits for" }
                }
            })),
        },
        Tool {
            name: "artcraft_get_subscription".to_string(),
            description: "Get subscription tier and limits.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {}
            })),
        },
        Tool {
            name: "artcraft_create_checkout_session".to_string(),
            description: "Create a Stripe checkout session to purchase credit packs when user credits are low.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "credits_pack": {
                        "type": "string",
                        "enum": [
                            "artcraft_1000",
                            "artcraft_2500",
                            "artcraft_5000",
                            "artcraft_10000",
                            "artcraft_25000",
                            "artcraft_50000"
                        ],
                        "description": "Credit pack size to purchase (default: artcraft_5000)"
                    }
                }
            })),
        },
        Tool {
            name: "artcraft_create_subscription_checkout".to_string(),
            description: "Create a Stripe subscription checkout link to upgrade or subscribe to ArtCraft Basic, Pro, or Max.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "plan": {
                        "type": "string",
                        "enum": ["artcraft_basic", "artcraft_pro", "artcraft_max"],
                        "description": "Subscription tier"
                    },
                    "cadence": {
                        "type": "string",
                        "enum": ["monthly", "yearly"],
                        "description": "Billing cycle (default: monthly)"
                    }
                },
                "required": ["plan"]
            })),
        },
        Tool {
            name: "artcraft_get_billing_portal_url".to_string(),
            description: "Get a Stripe customer billing portal URL where the user can manage cards, subscriptions, invoices, and plans.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {}
            })),
        },
        Tool {
            name: "artcraft_get_billing_cancel_plan_url".to_string(),
            description: "Get a Stripe customer portal URL preconfigured for cancelling the current ArtCraft plan. This only creates the portal link; cancellation still requires user confirmation in Stripe.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {}
            })),
        },
        Tool {
            name: "artcraft_get_billing_switch_plan_url".to_string(),
            description: "Get a Stripe customer portal URL preconfigured for switching to another ArtCraft subscription plan/cadence. This only creates the portal link; the plan change still requires user confirmation in Stripe.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "plan": { "type": "string", "enum": ["artcraft_basic", "artcraft_pro", "artcraft_max"] },
                    "cadence": { "type": "string", "enum": ["monthly", "yearly"], "description": "Billing cycle (default: monthly)" }
                },
                "required": ["plan"]
            })),
        },
        Tool {
            name: "artcraft_get_billing_update_payment_method_url".to_string(),
            description: "Get a Stripe customer portal URL preconfigured for updating the payment method. This only creates the portal link; payment details are entered by the user in Stripe.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {}
            })),
        },
        Tool {
            name: "artcraft_set_unsubscribe_reason".to_string(),
            description: "Submit optional user feedback explaining why they unsubscribed. Requires web-session cookie auth and records the reason upstream.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string", "minLength": 1 }
                },
                "required": ["reason"]
            })),
        },
    ]
}

pub async fn get_session_info(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let api_hostname = client.api_host.to_api_hostname_and_scheme();
    let url = format!("{}/v1/session", api_hostname);
    let response = raw_json_get(&url, client).await?;

    let logged_in = response
        .get("logged_in")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !logged_in {
        return Ok(vec![ToolContent {
            content_type: "text".to_string(),
            text: "Session info:\n  Logged in: false".to_string(),
        }]);
    }

    let user = response.get("user").cloned().unwrap_or_else(|| json!({}));
    let core_info = user.get("core_info").cloned().unwrap_or_else(|| json!({}));
    let username = user
        .get("username")
        .and_then(|v| v.as_str())
        .or_else(|| core_info.get("username").and_then(|v| v.as_str()))
        .unwrap_or("?");
    let display_name = user
        .get("display_name")
        .and_then(|v| v.as_str())
        .or_else(|| core_info.get("display_name").and_then(|v| v.as_str()))
        .unwrap_or(username);
    let user_token = user
        .get("user_token")
        .and_then(|v| v.as_str())
        .or_else(|| core_info.get("user_token").and_then(|v| v.as_str()))
        .unwrap_or("?");

    let text = format!(
        "Session info:\n  Logged in: true\n  User: {}\n  Display name: {}\n  Token: {}",
        username, display_name, user_token
    );

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn get_credits(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let namespace = arguments["namespace"].as_str().unwrap_or("artcraft");
    let url = client.absolute_url(&format!("/v1/credits/namespace/{}", namespace));
    let response = raw_json_get(&url, client).await?;

    let text = format!(
        "Credits info:
  Free credits: {}
  Monthly credits: {}
  Banked credits: {}
  Total: {}",
        response
            .get("free_credits")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        response
            .get("monthly_credits")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        response
            .get("banked_credits")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        response
            .get("sum_total_credits")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    );

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn get_subscription(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let url = client.absolute_url("/v1/subscriptions/namespace/artcraft");
    let response = raw_json_get(&url, client).await?;

    let text = if let Some(sub) = response.get("active_subscription") {
        if sub.is_null() {
            "No active subscription found.".to_string()
        } else {
            format!(
                "Subscription info:
  Plan: {}
  Namespace: {}
  Next bill: {}
  Ends at: {}",
                sub.get("product_slug")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                sub.get("namespace").and_then(|v| v.as_str()).unwrap_or("?"),
                sub.get("next_bill_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                sub.get("subscription_end_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
            )
        }
    } else {
        "No active subscription found.".to_string()
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn create_checkout_session(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let pack_str = arguments["credits_pack"]
        .as_str()
        .unwrap_or("artcraft_5000");
    let credits_pack = match pack_str {
        "artcraft_1000" => Some(ArtcraftCreditsPackSlug::Artcraft1000),
        "artcraft_2500" => Some(ArtcraftCreditsPackSlug::Artcraft2500),
        "artcraft_5000" => Some(ArtcraftCreditsPackSlug::Artcraft5000),
        "artcraft_10000" => Some(ArtcraftCreditsPackSlug::Artcraft10000),
        "artcraft_25000" => Some(ArtcraftCreditsPackSlug::Artcraft25000),
        "artcraft_50000" => Some(ArtcraftCreditsPackSlug::Artcraft50000),
        _ => Some(ArtcraftCreditsPackSlug::Artcraft5000),
    };

    let request = StripeArtcraftCreateCreditsPackCheckoutRequest { credits_pack };
    let response = typed_json_post(
        artcraft_api_defs::stripe_artcraft::create_credits_pack_checkout::CREATE_CREDITS_PACK_CHECKOUT_URL_PATH,
        request,
        client,
    ).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Stripe credit pack checkout session created successfully.\nDirect checkout URL: {}",
            response
                .get("stripe_checkout_redirect_url")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ),
    }])
}

pub async fn create_subscription_checkout(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let plan_str = arguments["plan"].as_str().ok_or_else(|| {
        anyhow!("plan is required ('artcraft_basic', 'artcraft_pro', 'artcraft_max')")
    })?;

    let plan = match plan_str {
        "artcraft_basic" => Some(ArtcraftSubscriptionSlug::ArtcraftBasic),
        "artcraft_pro" => Some(ArtcraftSubscriptionSlug::ArtcraftPro),
        "artcraft_max" => Some(ArtcraftSubscriptionSlug::ArtcraftMax),
        _ => return Err(anyhow!("Unknown subscription plan: {}", plan_str)),
    };

    let cadence = match arguments["cadence"].as_str().unwrap_or("monthly") {
        "yearly" => Some(PlanBillingCadence::Yearly),
        _ => Some(PlanBillingCadence::Monthly),
    };

    let request = StripeArtcraftCreateSubscriptionCheckoutRequest { plan, cadence };
    let response = typed_json_post(
        artcraft_api_defs::stripe_artcraft::create_subscription_checkout::CREATE_SUBSCRIPTION_CHECKOUT_URL_PATH,
        request,
        client,
    ).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Stripe subscription checkout session created successfully.\nDirect checkout URL: {}",
            response
                .get("stripe_checkout_redirect_url")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ),
    }])
}

async fn portal_url_response<T: serde::Serialize>(
    route_path: &str,
    request: T,
    client: &ArtCraftClient,
) -> Result<Value> {
    match typed_json_post(route_path, request, client).await {
        Ok(response) => Ok(response),
        Err(err) if err.to_string().contains("user has no active subscription") => Err(anyhow!(
            "No billing portal is available because this account has no active ArtCraft subscription."
        )),
        Err(err) => Err(err),
    }
}

fn portal_url_content(response: &Value, noun: &str) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Stripe customer portal URL retrieved successfully.\n{} URL: {}",
            noun,
            response
                .get("stripe_portal_url")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ),
    }]
}

pub async fn get_billing_portal_url(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let request = StripeArtcraftCustomerPortalManagePlanRequest {
        portal_config_id: None,
    };
    let response = match portal_url_response(
        artcraft_api_defs::stripe_artcraft::customer_portal_manage_plan::CUSTOMER_PORTAL_MANAGE_PLAN_URL_PATH,
        request,
        client,
    ).await {
        Ok(response) => response,
        Err(err) if err.to_string().contains("No billing portal is available") => {
            return Ok(vec![ToolContent {
                content_type: "text".to_string(),
                text: err.to_string(),
            }]);
        }
        Err(err) => return Err(err),
    };
    Ok(portal_url_content(&response, "Management"))
}

pub async fn get_billing_cancel_plan_url(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let request = StripeArtcraftCustomerPortalCancelPlanRequest {
        portal_config_id: None,
    };
    let response = portal_url_response(
        artcraft_api_defs::stripe_artcraft::customer_portal_cancel_plan::CUSTOMER_PORTAL_CANCEL_PLAN_URL_PATH,
        request,
        client,
    ).await?;
    Ok(portal_url_content(&response, "Cancellation"))
}

pub async fn get_billing_switch_plan_url(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let plan_str = arguments["plan"].as_str().ok_or_else(|| {
        anyhow!("plan is required ('artcraft_basic', 'artcraft_pro', 'artcraft_max')")
    })?;
    let plan = match plan_str {
        "artcraft_basic" => Some(ArtcraftSubscriptionSlug::ArtcraftBasic),
        "artcraft_pro" => Some(ArtcraftSubscriptionSlug::ArtcraftPro),
        "artcraft_max" => Some(ArtcraftSubscriptionSlug::ArtcraftMax),
        _ => return Err(anyhow!("Unknown subscription plan: {}", plan_str)),
    };
    let cadence = match arguments["cadence"].as_str().unwrap_or("monthly") {
        "yearly" => Some(PlanBillingCadenceConfirmation::Yearly),
        _ => Some(PlanBillingCadenceConfirmation::Monthly),
    };
    let request = StripeArtcraftCustomerPortalSwitchPlanRequest {
        portal_config_id: None,
        plan,
        cadence,
    };
    let response = portal_url_response(
        artcraft_api_defs::stripe_artcraft::customer_portal_switch_plan::CUSTOMER_PORTAL_SWITCH_PLAN_URL_PATH,
        request,
        client,
    ).await?;
    Ok(portal_url_content(&response, "Plan switching"))
}

pub async fn get_billing_update_payment_method_url(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let request = StripeArtcraftCustomerPortalUpdatePaymentMethodRequest {
        portal_config_id: None,
    };
    let response = portal_url_response(
        artcraft_api_defs::stripe_artcraft::customer_portal_update_payment_method::CUSTOMER_PORTAL_UPDATE_PAYMENT_METHOD_URL_PATH,
        request,
        client,
    ).await?;
    Ok(portal_url_content(&response, "Payment method update"))
}

pub async fn set_unsubscribe_reason(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let reason = arguments["reason"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("reason is required"))?;
    raw_json_post(
        &client.absolute_url("/v1/subscriptions/unsubscribe_reason"),
        json!({"reason": reason}),
        client,
    )
    .await?;
    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: "Unsubscribe reason submitted.".to_string(),
    }])
}
