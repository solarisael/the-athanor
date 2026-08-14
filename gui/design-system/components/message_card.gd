@tool
class_name MessageCard
extends VBoxContainer

## One entry in the S01 transcript: who spoke, when, what was said, and — only
## when the Host actually reports one — the tool call that speech performed.
##
## The scene ships no synthetic content. Every string arrives from a caller
## (a screen, or the Rust adoption wave). The single piece of fixed copy is the
## "TOOL CALL" heading, which is a scene constant rather than a prop, so no
## caller can relabel a tool call into something it is not (design lesson 297).
##
## Reading tiers (design lesson 302): the author name and the message body are
## required reading and use the 0.78+ tiers (AthanorStatusValue and
## AthanorMessageBody, both ink 0.876). The timestamp is decoration and sits in
## the muted AthanorMeta tier. No node here overrides a font (project lesson
## 375) or names a colour: every colour comes from a theme type variation.
##
## An empty `tool_call_text` keeps the tool-call sub-panel collapsed. There is
## deliberately no separate "show tool call" flag, because a visible but empty
## tool panel would claim a call that never happened (design lesson 294).

## Speaker name, exactly as the Host reports it.
@export var author: String = "":
	set(value):
		author = value
		if is_node_ready():
			_author_label.text = author

## Already-formatted timestamp. This component never formats or guesses time.
@export var timestamp: String = "":
	set(value):
		timestamp = value
		if is_node_ready():
			_timestamp_label.text = timestamp

## Message body. Rendered as plain text: BBCode stays off so Host content can
## never inject markup or colour into the transcript.
@export_multiline var body: String = "":
	set(value):
		body = value
		if is_node_ready():
			_body_label.text = body

## Tool call performed by this message. Empty collapses the sub-panel.
@export_multiline var tool_call_text: String = "":
	set(value):
		tool_call_text = value
		if is_node_ready():
			_apply_tool_call()

@onready var _author_label: Label = %Author
@onready var _timestamp_label: Label = %Timestamp
@onready var _body_label: RichTextLabel = %Body
@onready var _tool_call_surface: Control = %ToolCall
@onready var _tool_call_body: Label = %ToolCallBody


func _ready() -> void:
	_author_label.text = author
	_timestamp_label.text = timestamp
	_body_label.text = body
	_apply_tool_call()


## Fill every channel of the card in one call. Returns nothing: the card is the
## record, not a report about itself.
func fill(p_author: String, p_timestamp: String, p_body: String, p_tool_call_text: String = "") -> void:
	author = p_author
	timestamp = p_timestamp
	body = p_body
	tool_call_text = p_tool_call_text


func _apply_tool_call() -> void:
	_tool_call_body.text = tool_call_text
	_tool_call_surface.visible = not tool_call_text.is_empty()
