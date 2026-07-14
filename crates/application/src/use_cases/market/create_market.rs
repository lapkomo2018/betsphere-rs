use std::sync::Arc;

use chrono::{DateTime, Utc};
use domain::entities::{Market, Outcome};
use domain::repositories::MarketRepository;
use domain::services::{authorization, pricing};
use domain::value_objects::market::{MarketTitle, OutcomeLabel};

use super::MarketView;
use crate::{Actor, ApplicationError};

/// Validated inputs for creating a market. Labels drive the initial outcomes;
/// at least two are required.
pub struct NewMarket {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub closes_at: Option<DateTime<Utc>>,
    pub outcomes: Vec<String>,
}

/// Creates a market with its outcomes at even starting prices. Only actors
/// allowed by [`authorization::can_manage_markets`] may call this.
pub struct CreateMarket {
    markets: Arc<dyn MarketRepository>,
}

impl CreateMarket {
    pub fn new(markets: Arc<dyn MarketRepository>) -> Self {
        Self { markets }
    }

    pub async fn execute(
        &self,
        actor: &Actor,
        input: NewMarket,
    ) -> Result<MarketView, ApplicationError> {
        if !authorization::can_manage_markets(actor.role) {
            return Err(ApplicationError::Forbidden("admin role required".into()));
        }

        if input.outcomes.len() < 2 {
            return Err(ApplicationError::Domain(domain::DomainError::Validation(
                "a market needs at least two outcomes".into(),
            )));
        }

        let title = MarketTitle::new(input.title)?;
        let market = Market::new(title, input.description, input.category, input.closes_at);

        // Start every outcome at the even split, then let the pricing service
        // normalise so the prices sum to exactly 1.0000.
        let mut outcomes = input
            .outcomes
            .into_iter()
            .map(|label| {
                Ok(Outcome::new(
                    market.id(),
                    OutcomeLabel::new(label)?,
                    domain::value_objects::market::Price::ZERO,
                ))
            })
            .collect::<Result<Vec<_>, domain::DomainError>>()?;
        pricing::recalculate_prices(&mut outcomes);

        self.markets.create(&market, &outcomes).await?;

        Ok(MarketView { market, outcomes })
    }
}
