use serenity::all::MessageId;

use crate::{message_id_from_i64, queries::NewScore, LastMessage, ScoreImproved};

pub(crate) fn should_post_new_message(
    latest_message: Option<i32>,
    current_score: &ScoreImproved,
    previous_message_for_challenge: &Option<LastMessage>,
) -> Option<MessageId> {
    previous_message_for_challenge
        .as_ref()
        .and_then(|last_message_for_challenge| {
            latest_message
                .is_some_and(|e| {
                    e == last_message_for_challenge.id
                        && current_score.author == last_message_for_challenge.author_id
                })
                .then(|| message_id_from_i64(last_message_for_challenge.message_id))
        })
}

pub(crate) fn get_last_best_score_fields(
    previous_message_for_challenge: &Option<LastMessage>,
    latest_message: Option<i32>,
    curent_score: NewScore,
) -> NewScore {
    match previous_message_for_challenge {
        Some(previous_message)
            if Some(previous_message.id) == latest_message
                && previous_message.author_id == curent_score.user_id =>
        {
            NewScore {
                username: previous_message
                    .previous_author_name
                    .clone()
                    .unwrap_or(curent_score.username),
                score: previous_message
                    .previous_author_score
                    .unwrap_or(curent_score.score),
                user_id: previous_message
                    .previous_author_id
                    .unwrap_or(curent_score.user_id),
            }
        }
        Some(previous_message) => NewScore {
            user_id: previous_message.author_id,
            score: previous_message.score,
            username: previous_message.author_name.clone(),
        },
        None => curent_score,
    }
}
