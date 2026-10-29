-- Keep provider commentary in the durable run trace, not in the visible reply.
WITH final_text AS (
  SELECT r.id, string_agg(part->>'text', E'\n' ORDER BY item_order,part_order) AS text
  FROM runs r,
  LATERAL jsonb_array_elements(r.context) WITH ORDINALITY AS items(item,item_order),
  LATERAL jsonb_array_elements(CASE WHEN item->>'type'='message' THEN item->'content' ELSE '[]'::jsonb END) WITH ORDINALITY AS parts(part,part_order)
  WHERE r.status='succeeded' AND item->>'phase'='final_answer' AND part->>'type'='output_text'
  GROUP BY r.id
)
UPDATE runs SET answer=final_text.text FROM final_text WHERE runs.id=final_text.id;
UPDATE messages SET text=runs.answer FROM runs WHERE messages.id=runs.assistant_message_id AND runs.status='succeeded' AND runs.answer<>'';
