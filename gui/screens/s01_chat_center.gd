@tool
class_name S01ChatCenter
extends VBoxContainer

## S01 · CONVERSA E RETOMADA — the center instrument for the active conversation.
##
## Composition (top to bottom): PageHeader, the latest Paper Boat ReceiptCard,
## the transcript, the Composer docked at the bottom. The transcript owns its own
## ScrollContainer so the conversation scrolls while the header and composer stay
## put; when the adoption wave mounts this page inside the shell's center scroll,
## that outer scroll must be neutralised for this page or the two will fight.
##
## HONESTY: this scene contains no messages. The Host serves no chat projection
## yet, so an empty transcript shows the DisclosureBanner ABSENT_CONTRACT preset
## and nothing else (design lessons 297 and 301). Content enters only through
## `add_message` and `set_receipt`, called by a real data source. Any synthetic
## message belongs in a smoke script, never in this scene.

## The operator asked to send this exact text. Nothing has been sent yet: the
## caller owns the send, and the draft is still in the composer.
signal message_submitted(text: String)

## The operator abandoned the draft; the composer has already cleared it.
signal draft_cancelled()

const MESSAGE_CARD: PackedScene = preload("res://design-system/components/message_card.tscn")

## Every field `set_receipt` accepts. These are exactly the ReceiptCard exports;
## keys outside this set are refused loudly instead of silently dropped.
const RECEIPT_FIELDS: PackedStringArray = [
	"title_text",
	"timestamp_text",
	"delivered_text",
	"record_text",
	"event_text",
	"sequence_text",
	"sha_text",
]

@onready var _receipt: ReceiptCard = %Receipt
@onready var _transcript: ScrollContainer = %Transcript
@onready var _messages: VBoxContainer = %Messages
@onready var _empty_state: DisclosureBanner = %EmptyState
@onready var _composer: Composer = %Composer


func _ready() -> void:
	_composer.submitted.connect(_on_composer_submitted)
	_composer.cancelled.connect(_on_composer_cancelled)
	_apply_empty_state()


## Append one message to the transcript and return the card, so the caller can
## keep the handle (streaming updates, later tool-call attachment). All four
## strings are rendered verbatim; an empty `tool_call_text` leaves the card's
## tool-call sub-panel collapsed.
func add_message(author: String, timestamp: String, body: String, tool_call_text: String = "") -> MessageCard:
	var card: MessageCard = MESSAGE_CARD.instantiate()
	_messages.add_child(card)
	card.fill(author, timestamp, body, tool_call_text)
	_apply_empty_state()
	scroll_to_latest()
	return card


## Remove every message and restore the honest empty state.
func clear_transcript() -> void:
	for card: Node in _messages.get_children():
		_messages.remove_child(card)
		card.queue_free()
	_apply_empty_state()


## How many messages the transcript currently holds.
func message_count() -> int:
	return _messages.get_child_count()


## Write the latest Paper Boat receipt. Keys must come from RECEIPT_FIELDS;
## omitted fields keep their current value, so a partial update is allowed but a
## misspelled key is reported rather than ignored.
func set_receipt(fields: Dictionary) -> void:
	for key in fields:
		var field_name: String = str(key)
		if not RECEIPT_FIELDS.has(field_name):
			push_warning("S01ChatCenter.set_receipt: unknown receipt field '%s'" % field_name)
			continue
		_receipt.set(field_name, str(fields[key]))


## Reason the composer currently refuses to submit, or an empty string when Send
## is live. Setting a non-empty reason both disables Send and renders the reason.
func set_submit_enabled_reason(reason: String) -> void:
	_composer.submit_enabled_reason = reason


## Bring the newest message into view.
func scroll_to_latest() -> void:
	_transcript.set_deferred(&"scroll_vertical", 1 << 24)


func _on_composer_submitted(text: String) -> void:
	message_submitted.emit(text)


func _on_composer_cancelled() -> void:
	draft_cancelled.emit()


func _apply_empty_state() -> void:
	_empty_state.visible = _messages.get_child_count() == 0
