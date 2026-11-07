//! Port of `MessageDisplayItem.ExtractImages` / `ExtractReactions`. The legacy
//! .NET code used cached reflection to probe `Attachments`/`Blocks`/`Reactions`
//! because SlackNet's typed model wasn't stable across those fields; here we
//! deserialize into permissive `serde_json::Value`s and probe fields directly,
//! which is the natural Rust equivalent (no reflection needed).

use super::dto::SlackMessage;
use super::models::{MessageImageItem, MessageReactionItem};

/// Field probing order for attachments/blocks, checked both on the object
/// itself and on a nested `image` sub-object.
const ATTACHMENT_IMAGE_FIELDS: [&str; 5] =
    ["image_url", "image_original", "thumbnail_url", "thumb_url", "url"];

pub fn extract_images(message: &SlackMessage) -> Vec<MessageImageItem> {
    let mut images = Vec::new();

    for file in &message.files {
        let is_image = file
            .mimetype
            .as_deref()
            .map(|m| m.to_ascii_lowercase().starts_with("image/"))
            .unwrap_or(false);
        if !is_image {
            continue;
        }
        images.push(MessageImageItem::new(vec![
            file.thumb_360.as_deref(),
            file.thumb_480.as_deref(),
            file.thumb_160.as_deref(),
            file.thumb_80.as_deref(),
            file.thumb_64.as_deref(),
            file.url_private_download.as_deref(),
            file.url_private.as_deref(),
        ]));
    }

    for attachment in &message.attachments {
        push_value_image_candidates(&mut images, attachment);
    }

    for block in &message.blocks {
        push_value_image_candidates(&mut images, block);
    }

    images.retain(|item| !item.candidate_urls.is_empty());
    images
}

fn push_value_image_candidates(images: &mut Vec<MessageImageItem>, value: &serde_json::Value) {
    images.push(image_item_from_value(value));
    if let Some(nested) = value.get("image") {
        images.push(image_item_from_value(nested));
    }
}

fn image_item_from_value(value: &serde_json::Value) -> MessageImageItem {
    let candidates: Vec<Option<&str>> = ATTACHMENT_IMAGE_FIELDS
        .iter()
        .map(|field| value.get(*field).and_then(|v| v.as_str()))
        .collect();
    MessageImageItem::new(candidates)
}

pub fn extract_reactions(message: &SlackMessage) -> Vec<MessageReactionItem> {
    message
        .reactions
        .iter()
        .filter_map(|reaction| {
            let name = reaction.name.as_ref()?;
            if name.trim().is_empty() {
                return None;
            }
            Some(MessageReactionItem {
                name: name.clone(),
                count: reaction.count as i32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message_with(files: Vec<super::super::dto::SlackFile>) -> SlackMessage {
        SlackMessage {
            files,
            ..Default::default()
        }
    }

    #[test]
    fn non_image_files_are_skipped() {
        let file = super::super::dto::SlackFile {
            mimetype: Some("application/pdf".to_string()),
            url_private: Some("https://files.slack.com/doc.pdf".to_string()),
            ..Default::default()
        };
        let message = message_with(vec![file]);
        assert!(extract_images(&message).is_empty());
    }

    #[test]
    fn image_file_collects_candidates_in_priority_order() {
        let file = super::super::dto::SlackFile {
            mimetype: Some("image/png".to_string()),
            thumb_360: Some("https://x/360.png".to_string()),
            url_private: Some("https://x/private.png".to_string()),
            ..Default::default()
        };
        let message = message_with(vec![file]);
        let images = extract_images(&message);
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].candidate_urls,
            vec!["https://x/360.png", "https://x/private.png"]
        );
    }

    #[test]
    fn attachment_image_url_is_extracted_including_nested_image_object() {
        let mut message = SlackMessage::default();
        message.attachments = vec![json!({
            "image_url": "https://x/attach.png",
            "image": { "image_url": "https://x/nested.png" }
        })];
        let images = extract_images(&message);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].candidate_urls, vec!["https://x/attach.png"]);
        assert_eq!(images[1].candidate_urls, vec!["https://x/nested.png"]);
    }

    #[test]
    fn reactions_keep_only_name_and_count() {
        let mut message = SlackMessage::default();
        message.reactions = vec![
            super::super::dto::SlackReaction {
                name: Some("thumbsup".to_string()),
                count: 3,
                users: vec!["U1".to_string()],
            },
            super::super::dto::SlackReaction {
                name: None,
                count: 1,
                users: vec![],
            },
        ];
        let reactions = extract_reactions(&message);
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].name, "thumbsup");
        assert_eq!(reactions[0].count, 3);
    }
}
