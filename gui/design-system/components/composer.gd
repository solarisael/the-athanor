@tool
class_name Composer
extends VBoxContainer

## The breath of S01: where the operator writes, and the two consequences that
## writing can have — send it (durable) or abandon it (cancel).
##
## The input grows with the text, from MIN_LINES to MAX_LINES, then scrolls. It
## never jumps to full height for one line, and never eats the transcript.
##
## Submission is governed by `submit_enabled_reason` (design lesson 295): there
## is no bare disabled flag anywhere here. A non-empty reason both disables Send
## and is rendered by the Send button as the reason it cannot be pressed. When
## the caller has no objection, the composer still refuses to submit blank text
## and says so with EMPTY_INPUT_REASON, so "send nothing" is not a constructible
## state (design lesson 294).
##
## Colours and sizes come from theme type variations only (AthanorComposerField
## for the input); no node here overrides a font (project lesson 375).

## Emitted with the exact text the operator wrote. The composer does not clear
## itself on submit: only the caller knows whether the send was accepted.
signal submitted(text: String)

## Emitted after the composer has cleared the abandoned draft.
signal cancelled()

const MIN_LINES: int = 3
const MAX_LINES: int = 8

## Fixed copy, not a prop: the composer's own objection to an empty draft
## (design lesson 297).
const EMPTY_INPUT_REASON: String = "Nothing written yet."

## Empty means the caller permits submission. Non-empty disables Send and is
## shown verbatim as the reason.
@export var submit_enabled_reason: String = "":
	set(value):
		submit_enabled_reason = value
		if is_node_ready():
			_apply_submit_state()

## Prompt shown while the draft is empty.
@export var placeholder: String = "":
	set(value):
		placeholder = value
		if is_node_ready():
			_input.placeholder_text = placeholder

@onready var _input: TextEdit = %Input
@onready var _send: ConsequenceButton = %Send
@onready var _cancel: TextAction = %Cancel


func _ready() -> void:
	_input.placeholder_text = placeholder
	_input.text_changed.connect(_on_text_changed)
	_send.pressed.connect(_on_send_pressed)
	_cancel.pressed.connect(_on_cancel_pressed)
	_apply_input_height()
	_apply_submit_state()


## Current draft, unmodified.
func get_draft() -> String:
	return _input.text


## Replace the draft — used when a caller restores or seeds an edit, never to
## fake operator input.
func set_draft(text: String) -> void:
	_input.text = text
	_input.set_caret_line(_input.get_line_count() - 1)
	_input.set_caret_column(_input.get_line(_input.get_line_count() - 1).length())
	_apply_input_height()
	_apply_submit_state()


## Drop the draft without emitting `cancelled` — for callers that consumed it.
func clear_draft() -> void:
	_input.text = ""
	_apply_input_height()
	_apply_submit_state()


## The reason submission is currently refused, or an empty string when Send is
## live. Callers can read this instead of duplicating the rule.
func submit_refusal() -> String:
	if not submit_enabled_reason.is_empty():
		return submit_enabled_reason
	if _input.text.strip_edges().is_empty():
		return EMPTY_INPUT_REASON
	return ""


func _on_text_changed() -> void:
	_apply_input_height()
	_apply_submit_state()


func _on_send_pressed() -> void:
	if not submit_refusal().is_empty():
		return
	submitted.emit(_input.text)


func _on_cancel_pressed() -> void:
	clear_draft()
	cancelled.emit()


func _apply_submit_state() -> void:
	_send.disabled_reason = submit_refusal()


func _apply_input_height() -> void:
	var rows: int = 0
	for line: int in _input.get_line_count():
		rows += _input.get_line_wrap_count(line) + 1
	rows = clampi(rows, MIN_LINES, MAX_LINES)
	var chrome: float = 0.0
	var frame: StyleBox = _input.get_theme_stylebox(&"normal")
	if frame != null:
		chrome = frame.get_minimum_size().y
	_input.custom_minimum_size.y = float(rows * _input.get_line_height()) + chrome
