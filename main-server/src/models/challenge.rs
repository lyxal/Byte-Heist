use common::RunLangOutput;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::Error;

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct NewChallenge {
    pub description: String,
    pub judge: String,
    pub name: String,
    pub example_code: String,
}

impl Default for NewChallenge {
    fn default() -> Self {
        NewChallenge {
            description: concat!(
                "Explain in detail how to solve your challenge. Good challenge descriptions ",
                "include examples and links to relevent resources. Markdown is supported"
            )
            .to_string(),
            judge: concat!(
                "(async function*(code: Code): AsyncGenerator<TestCase, FinalVerdict, undefined> {\n",
                "  yield (await code.run(undefined)).assertEquals('Hello World!');\n",
                "  //Your code here\n",
                "  return code.noFailures();\n",
                "})"
            )
            .to_string(),
            name: String::new(),
            example_code: String::new(),
        }
    }
}

#[derive(Serialize)]
pub struct NewChallengeWithTests {
    #[serde(flatten)]
    pub challenge: NewChallenge,
    pub tests: Option<RunLangOutput>,
}

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct Challenge {
    pub id: i32,
    #[sqlx(flatten)]
    pub challenge: NewChallenge,
    pub author: i32,
}

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct ChallengeWithAuthorInfo {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub challenge: Challenge,
    pub author_name: String,
    pub author_avatar: String,
}

impl ChallengeWithAuthorInfo {
    pub async fn get_by_id(pool: &PgPool, id: i32) -> Result<Option<Self>, Error> {
        let sql = "SELECT
            challenges.id,
            challenges.name,
            challenges.description,
            challenges.judge,
            challenges.example_code,
            challenges.author,
            accounts.username as author_name,
            accounts.avatar as author_avatar
            FROM challenges LEFT JOIN accounts ON challenges.author = accounts.id
            WHERE challenges.id=$1
            "
        .to_string();

        let challenge: Option<ChallengeWithAuthorInfo> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(Error::DatabaseError)?;

        Ok(challenge)
    }
}
