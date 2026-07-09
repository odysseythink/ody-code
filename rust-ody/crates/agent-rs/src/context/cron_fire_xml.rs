use crate::cron::types::CronFireContext;

pub fn render_cron_fire_xml(ctx: &CronFireContext) -> String {
    format!(
        "<cron_fire>\n  <id>{id}</id>\n  <schedule>{schedule}</schedule>\n  <prompt>{prompt}</prompt>\n  <coalesced_count>{coalesced_count}</coalesced_count>\n  <fired_at>{fired_at}</fired_at>\n</cron_fire>",
        id = ctx.id,
        schedule = xml_escape(&ctx.schedule),
        prompt = xml_escape(&ctx.prompt),
        coalesced_count = ctx.coalesced_count,
        fired_at = ctx.fired_at.to_rfc3339(),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::types::CronTaskId;
    use chrono::Utc;

    #[test]
    fn renders_cron_fire_xml() {
        let ctx = CronFireContext {
            id: CronTaskId::new("cron-42"),
            schedule: "*/5 * * * *".to_string(),
            prompt: "Check CI status".to_string(),
            coalesced_count: 3,
            fired_at: Utc::now(),
        };
        let xml = render_cron_fire_xml(&ctx);
        assert!(xml.contains("<cron_fire>"));
        assert!(xml.contains("<id>cron-42</id>"));
        assert!(xml.contains("<schedule>*/5 * * * *</schedule>"));
        assert!(xml.contains("<prompt>Check CI status</prompt>"));
        assert!(xml.contains("<coalesced_count>3</coalesced_count>"));
        assert!(xml.contains("<fired_at>"));
    }
}
