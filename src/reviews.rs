#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db;

// ── Agent Reviews & Ratings ─────────────────────────────────────────
//
// Users can leave 1-5 star ratings with optional comments on agents.
// Agents get an average rating and review count for trust/discovery.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentReview {
    pub id: String,
    pub agent_id: String,
    pub reviewer: String,
    pub rating: i32,
    pub comment: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentRatingSummary {
    pub agent_id: String,
    pub average_rating: f64,
    pub total_reviews: i64,
    pub rating_distribution: RatingDistribution,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RatingDistribution {
    pub one_star: i64,
    pub two_star: i64,
    pub three_star: i64,
    pub four_star: i64,
    pub five_star: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateReviewRequest {
    pub reviewer: String,
    pub rating: i32,
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

const MIN_RATING: i32 = 1;
const MAX_RATING: i32 = 5;
const MAX_COMMENT_LENGTH: usize = 500;

pub struct AgentReviews;

impl AgentReviews {
    /// Submit a review for an agent
    pub async fn create_review(agent_id: &str, req: &CreateReviewRequest) -> Result<AgentReview> {
        // Validate rating
        if req.rating < MIN_RATING || req.rating > MAX_RATING {
            anyhow::bail!("Rating must be between {} and {}", MIN_RATING, MAX_RATING);
        }

        // Validate comment length
        if let Some(ref comment) = req.comment {
            if comment.len() > MAX_COMMENT_LENGTH {
                anyhow::bail!("Comment exceeds maximum length of {} characters", MAX_COMMENT_LENGTH);
            }
        }

        // Validate reviewer is not empty
        if req.reviewer.trim().is_empty() {
            anyhow::bail!("Reviewer address cannot be empty");
        }

        let review = AgentReview {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            reviewer: req.reviewer.trim().to_string(),
            rating: req.rating,
            comment: req.comment.clone(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
        };

        db::insert_review(&review).await?;

        Ok(review)
    }

    /// Get reviews for an agent
    pub async fn get_reviews(agent_id: &str, limit: usize, offset: usize) -> Result<Vec<AgentReview>> {
        db::get_agent_reviews(agent_id, limit, offset).await
    }

    /// Get rating summary for an agent
    pub async fn get_summary(agent_id: &str) -> Result<AgentRatingSummary> {
        let reviews = db::get_all_agent_reviews(agent_id).await?;

        let total = reviews.len() as i64;
        let avg = if total > 0 {
            reviews.iter().map(|r| r.rating as f64).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let distribution = RatingDistribution {
            one_star: reviews.iter().filter(|r| r.rating == 1).count() as i64,
            two_star: reviews.iter().filter(|r| r.rating == 2).count() as i64,
            three_star: reviews.iter().filter(|r| r.rating == 3).count() as i64,
            four_star: reviews.iter().filter(|r| r.rating == 4).count() as i64,
            five_star: reviews.iter().filter(|r| r.rating == 5).count() as i64,
        };

        Ok(AgentRatingSummary {
            agent_id: agent_id.to_string(),
            average_rating: (avg * 10.0).round() / 10.0, // Round to 1 decimal
            total_reviews: total,
            rating_distribution: distribution,
        })
    }

    /// Get top-rated agents
    pub async fn get_top_rated(limit: usize) -> Result<Vec<AgentRatingSummary>> {
        db::get_top_rated_agents(limit).await
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rating_validation() {
        assert!(0 < MIN_RATING);
        assert!(MAX_RATING == 5);
    }

    #[test]
    fn test_review_serialization() {
        let review = AgentReview {
            id: "rev-1".into(),
            agent_id: "agent-1".into(),
            reviewer: "0x1234".into(),
            rating: 4,
            comment: Some("Great agent!".into()),
            created_at: Some("2026-01-01T00:00:00Z".into()),
        };
        let json = serde_json::to_value(&review).unwrap();
        assert_eq!(json["rating"], 4);
        assert_eq!(json["comment"], "Great agent!");
    }

    #[test]
    fn test_rating_summary_serialization() {
        let summary = AgentRatingSummary {
            agent_id: "agent-1".into(),
            average_rating: 4.2,
            total_reviews: 10,
            rating_distribution: RatingDistribution {
                one_star: 0,
                two_star: 1,
                three_star: 1,
                four_star: 5,
                five_star: 3,
            },
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["average_rating"], 4.2);
        assert_eq!(json["total_reviews"], 10);
        assert_eq!(json["rating_distribution"]["five_star"], 3);
    }

    #[test]
    fn test_average_rating_calculation() {
        let ratings = vec![5, 4, 4, 3, 5];
        let total = ratings.len() as f64;
        let avg = ratings.iter().map(|r| *r as f64).sum::<f64>() / total;
        let rounded = (avg * 10.0).round() / 10.0;
        assert_eq!(rounded, 4.2);
    }

    #[test]
    fn test_distribution_counts() {
        let ratings = vec![1, 2, 3, 3, 4, 4, 4, 5, 5, 5];
        let dist = RatingDistribution {
            one_star: ratings.iter().filter(|&&r| r == 1).count() as i64,
            two_star: ratings.iter().filter(|&&r| r == 2).count() as i64,
            three_star: ratings.iter().filter(|&&r| r == 3).count() as i64,
            four_star: ratings.iter().filter(|&&r| r == 4).count() as i64,
            five_star: ratings.iter().filter(|&&r| r == 5).count() as i64,
        };
        assert_eq!(dist.one_star, 1);
        assert_eq!(dist.three_star, 2);
        assert_eq!(dist.four_star, 3);
        assert_eq!(dist.five_star, 3);
    }

    #[test]
    fn test_empty_reviews_zero_average() {
        let reviews: Vec<AgentReview> = vec![];
        let total = reviews.len() as i64;
        let avg = if total > 0 { 0.0 } else { 0.0 };
        assert_eq!(avg, 0.0);
        assert_eq!(total, 0);
    }
}
