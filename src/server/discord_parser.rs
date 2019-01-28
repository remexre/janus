use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use serenity::model::channel::Message;

use super::discord_side::ID_TO_NICK;

lazy_static! {
    static ref MENTION_PATTERN: Regex = Regex::new(r"<@!?(?P<id>[0-9]+)>").unwrap();
}

fn find_nickname(id: &u64, message: &Message) -> Option<String> {
    for user in &message.mentions {
        if *user.id.as_u64() == *id {
            {
                let mut map = ID_TO_NICK.write();
                map.insert(*id, user.name.clone());
            }
            return Some(format!("@{}", user.name));
        }
    }
    None
}

pub fn get_content(message: &Message) -> String {
    let other_content = MENTION_PATTERN.split(&message.content).map(str::to_owned);
    let mentions = MENTION_PATTERN
        .captures_iter(&message.content)
        .map(|captures| captures["id"].parse::<u64>().unwrap())
        .map(|id| {
            ID_TO_NICK
                .read()
                .get(&id)
                .map(String::clone)
                .or_else(|| find_nickname(&id, message))
                .unwrap_or_else(|| format!("<@{}>", id))
        });
    other_content.interleave(mentions).join("")
}
