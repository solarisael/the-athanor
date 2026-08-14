extends SceneTree

## Headless smoke for S01 · CONVERSA E RETOMADA.
##
## Instantiates the real screen, drives its public GDScript surface exactly as
## the Rust adoption wave will, then measures the laid-out geometry. Every
## synthetic message lives HERE and only here: the scene itself ships no
## conversation (design lesson 297).
##
## Two frames, on purpose: with a scripted SceneTree, `root` is not inside the
## tree during `_initialize` (nodes added there never receive `_ready`), and
## container layout only settles on the frame after the tree is built. Frame 1
## builds and exercises behaviour; frame 2 measures.
##
## Run:
##   athanor-gui.exe --headless --path <gui> --script res://screens/tests/s01_chat_center_smoke.gd

const SCREEN: String = "res://screens/s01_chat_center.tscn"

## The real center-instrument width at 1440 px wide: 1440 − 244 left rail −
## 308 right panel − 2 × 24 content margin. Measured at a cramped height first,
## then at a tall one, because the transcript must shrink AND breathe.
const CENTER_SIZE: Vector2 = Vector2(840, 700)
const TALL_CENTER_SIZE: Vector2 = Vector2(840, 1000)

const INK: Color = Color(0.87692922, 0.85448235, 0.81821482, 1)
const GOLD: Color = Color(0.76193195, 0.64497094, 0.44939537, 1)

var _frame: int = 0
var _failures: PackedStringArray = PackedStringArray()
var _screen: S01ChatCenter = null
var _stage: Control = null
var _cramped_transcript_height: float = 0.0


func _process(_delta: float) -> bool:
	_frame += 1
	if _frame == 1:
		if not _build():
			_report()
			return true
		_exercise()
		return false
	if _frame == 2:
		_cramped_transcript_height = _measure(CENTER_SIZE)
		_stage.size = TALL_CENTER_SIZE
		return false
	var tall_transcript_height: float = _measure(TALL_CENTER_SIZE)
	if tall_transcript_height <= _cramped_transcript_height:
		_failures.append("transcript must take the extra height when the center is taller")
	_report()
	return true


func _build() -> bool:
	var packed: PackedScene = load(SCREEN)
	if packed == null:
		_failures.append("could not load %s" % SCREEN)
		return false

	# Mount under the project theme exactly as main.tscn does, so every
	# theme_type_variation in the screen resolves for real, and at the real
	# center-instrument size so the measured geometry means something.
	_stage = Control.new()
	_stage.theme = load("res://theme/athanor_theme.tres")
	root.add_child(_stage)
	_stage.size = CENTER_SIZE

	_screen = packed.instantiate()
	_stage.add_child(_screen)
	if not _screen.is_node_ready():
		_failures.append("screen never became ready")
		return false

	print("screen class: %s" % _screen.get_script().get_global_name())
	return true


func _exercise() -> void:
	var empty_state: DisclosureBanner = _screen.get_node("%EmptyState")
	print("empty transcript: message_count=%d empty_state_visible=%s preset=%d" % [
		_screen.message_count(),
		str(empty_state.visible),
		empty_state.preset,
	])
	print("empty copy: %s" % empty_state.get_node("%Copy").text)
	if _screen.message_count() != 0:
		_failures.append("fresh scene must ship zero messages")
	if not empty_state.visible:
		_failures.append("empty transcript must show the ABSENT_CONTRACT disclosure")
	if empty_state.preset != DisclosureBanner.Preset.ABSENT_CONTRACT:
		_failures.append("empty state must use the ABSENT_CONTRACT preset")
	if empty_state.get_node("%Copy").text != "This surface's Host contract is not yet served. Nothing here is synthesized.":
		_failures.append("empty state must render the fixed ABSENT_CONTRACT sentence verbatim")

	_screen.add_message("Sol", "2026-08-14 21:04", "Onde paramos ontem?")
	_screen.add_message("Kodo", "2026-08-14 21:05", "No S01: transcript, composer, receipt.", "recall(query=\"onde paramos\") → 3 candidates")
	var third: MessageCard = _screen.add_message("Sol", "2026-08-14 21:06", "Então constrói a câmara.")

	print("after add_message x3: message_count=%d empty_state_visible=%s" % [
		_screen.message_count(),
		str(_screen.get_node("%EmptyState").visible),
	])
	print("messages children: %d" % _screen.get_node("%Messages").get_child_count())
	print("card 3 author=%s timestamp=%s body=%s tool_call_visible=%s" % [
		third.author,
		third.timestamp,
		third.get_node("%Body").text,
		str(third.get_node("%ToolCall").visible),
	])
	var second: MessageCard = _screen.get_node("%Messages").get_child(1)
	print("card 2 tool_call_visible=%s tool_call_text=%s" % [
		str(second.get_node("%ToolCall").visible),
		second.get_node("%ToolCallBody").text,
	])

	if _screen.message_count() != 3:
		_failures.append("three add_message calls must yield three cards")
	if _screen.get_node("%EmptyState").visible:
		_failures.append("empty state must hide once the transcript has messages")
	if third.get_node("%Body").text != "Então constrói a câmara.":
		_failures.append("body must render verbatim")
	if third.get_node("%ToolCall").visible:
		_failures.append("card without tool_call_text must keep its tool panel collapsed")
	if not second.get_node("%ToolCall").visible:
		_failures.append("card with tool_call_text must reveal its tool panel")

	# The two theme variations this wave added must resolve, or the transcript
	# body would fall back to Godot-default white and the composer would render
	# unstyled — both off-palette (design lesson 298).
	var body_node: RichTextLabel = third.get_node("%Body")
	var body_color: Color = body_node.get_theme_color(&"default_color")
	var body_size: int = body_node.get_theme_font_size(&"normal_font_size")
	print("body token: default_color=%s normal_font_size=%d font_from_theme=%s" % [
		str(body_color),
		body_size,
		str(body_node.get_theme_font(&"normal_font") != null),
	])
	if not body_color.is_equal_approx(INK):
		_failures.append("AthanorMessageBody must give the transcript body the ink tier, not white")
	if body_size < 14:
		_failures.append("transcript body must not drop below the 14px floor")

	_screen.set_receipt({
		"title_text": "Latest Paper Boat",
		"timestamp_text": "2026-08-14 20:58 · ROOM kodo",
		"delivered_text": "DELIVERED",
		"record_text": "memory 3536",
		"event_text": "boat.written",
		"sequence_text": "SEQ 0041",
		"sha_text": "1f440df6",
	})
	print("receipt title=%s timestamp=%s sha=%s" % [
		_screen.get_node("%Receipt").title_text,
		_screen.get_node("%Receipt").timestamp_text,
		_screen.get_node("%Receipt").sha_text,
	])
	if _screen.get_node("%Receipt").sha_text != "1f440df6":
		_failures.append("set_receipt must reach the ReceiptCard props")

	var composer: Composer = _screen.get_node("%Composer")
	var input: TextEdit = composer.get_node("%Input")
	print("composer token: font_color=%s caret=%s font_size=%d framed=%s" % [
		str(input.get_theme_color(&"font_color")),
		str(input.get_theme_color(&"caret_color")),
		input.get_theme_font_size(&"font_size"),
		str(input.get_theme_stylebox(&"normal") is StyleBoxFlat),
	])
	if not input.get_theme_color(&"font_color").is_equal_approx(INK):
		_failures.append("AthanorComposerField must give the input the ink tier")
	if not input.get_theme_color(&"caret_color").is_equal_approx(GOLD):
		_failures.append("composer caret must be the gold accent")
	if not (input.get_theme_stylebox(&"normal") is StyleBoxFlat):
		_failures.append("composer input must be framed by the theme, not left unstyled")
	if input.get_theme_font_size(&"font_size") < 14:
		_failures.append("composer input must not drop below the 14px floor")

	print("composer blank refusal=%s" % composer.submit_refusal())
	if composer.submit_refusal() != Composer.EMPTY_INPUT_REASON:
		_failures.append("blank draft must refuse submission with the fixed reason")

	composer.set_draft("uma linha")
	var one_line_height: float = input.custom_minimum_size.y
	print("composer 1-line height=%.1f refusal='%s'" % [one_line_height, composer.submit_refusal()])
	if not composer.submit_refusal().is_empty():
		_failures.append("a written draft must be submittable")

	composer.set_draft("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12")
	var many_line_height: float = input.custom_minimum_size.y
	var line_height: int = input.get_line_height()
	print("composer 12-line height=%.1f (line_height=%d, min=%d lines, max=%d lines)" % [
		many_line_height,
		line_height,
		Composer.MIN_LINES,
		Composer.MAX_LINES,
	])
	if many_line_height <= one_line_height:
		_failures.append("input must grow with the draft")
	if one_line_height < float(Composer.MIN_LINES * line_height):
		_failures.append("input must never be shorter than MIN_LINES")
	if many_line_height > float(Composer.MAX_LINES * line_height) + 64.0:
		_failures.append("input must stop growing at MAX_LINES")

	var submitted: Array[String] = []
	composer.submitted.connect(func(text: String) -> void: submitted.append(text))
	var forwarded: Array[String] = []
	_screen.message_submitted.connect(func(text: String) -> void: forwarded.append(text))
	composer.set_draft("enviar isto")
	composer.get_node("%Send").emit_signal("pressed")
	print("submit forwarded: composer=%s screen=%s" % [str(submitted), str(forwarded)])
	if submitted.size() != 1 or submitted[0] != "enviar isto":
		_failures.append("Send must emit submitted with the exact draft")
	if forwarded.size() != 1 or forwarded[0] != "enviar isto":
		_failures.append("screen must re-emit message_submitted")

	_screen.set_submit_enabled_reason("The Host serves no chat contract yet.")
	print("composer gated refusal=%s send_disabled=%s reason_visible=%s" % [
		composer.submit_refusal(),
		str(composer.get_node("%Send").disabled),
		str(composer.get_node("%Send").get_node("%DisabledReason").visible),
	])
	if not composer.get_node("%Send").disabled:
		_failures.append("a non-empty submit_enabled_reason must disable Send")
	if not composer.get_node("%Send").get_node("%DisabledReason").visible:
		_failures.append("a refused Send must render the reason (lesson 295)")

	_screen.clear_transcript()
	print("after clear_transcript: message_count=%d empty_state_visible=%s" % [
		_screen.message_count(),
		str(_screen.get_node("%EmptyState").visible),
	])
	if _screen.message_count() != 0 or not _screen.get_node("%EmptyState").visible:
		_failures.append("clear_transcript must restore the honest empty state")

	# Leave a populated transcript for the geometry frame.
	_screen.set_submit_enabled_reason("")
	_screen.get_node("%Composer").clear_draft()
	for index: int in 6:
		_screen.add_message("Sol", "2026-08-14 21:0%d" % index, "Linha de conversa %d, longa o bastante para exercitar o wrap do corpo da mensagem dentro da coluna central do instrumento." % index)


func _measure(center: Vector2) -> float:
	var header: Control = _screen.get_node("Header")
	var receipt: Control = _screen.get_node("%Receipt")
	var transcript: ScrollContainer = _screen.get_node("%Transcript")
	var composer: Control = _screen.get_node("%Composer")
	var messages: Control = _screen.get_node("%Messages")
	var first_card: Control = messages.get_child(0)

	print("geometry at center %dx%d: screen=%s" % [
		int(center.x),
		int(center.y),
		str(_screen.size),
	])
	print("  header y=%.0f h=%.0f" % [header.position.y, header.size.y])
	print("  receipt y=%.0f h=%.0f w=%.0f" % [receipt.position.y, receipt.size.y, receipt.size.x])
	print("  transcript y=%.0f h=%.0f w=%.0f" % [transcript.position.y, transcript.size.y, transcript.size.x])
	print("  composer y=%.0f h=%.0f bottom=%.0f" % [composer.position.y, composer.size.y, composer.position.y + composer.size.y])
	print("  first card w=%.0f h=%.0f" % [first_card.size.x, first_card.size.y])

	if not (header.position.y < receipt.position.y and receipt.position.y < transcript.position.y and transcript.position.y < composer.position.y):
		_failures.append("order must be header, receipt, transcript, composer")
	if _screen.size.y - (composer.position.y + composer.size.y) > 2.0:
		_failures.append("composer must sit AT the bottom, not float above it")
	# The page must FIT the center it is given: the transcript takes the leftover
	# space and shrinks, so the composer stays visibly docked instead of being
	# pushed below the viewport for the operator to go hunting for.
	if _screen.size.y > center.y + 1.0:
		_failures.append("page must not overflow the %.0f px center (got %.0f)" % [center.y, _screen.size.y])
	if composer.position.y + composer.size.y > center.y + 1.0:
		_failures.append("composer must stay inside the center viewport's bottom edge")
	if transcript.size.y < 120.0:
		_failures.append("transcript must keep at least its 120 px floor")
	if absf(transcript.size.x - center.x) > 1.0:
		_failures.append("transcript must span the full center column")
	if first_card.size.x > transcript.size.x:
		_failures.append("message cards must not overflow the center column")
	if first_card.size.y <= 0.0:
		_failures.append("message cards must have real height")
	return transcript.size.y


func _report() -> void:
	if _failures.is_empty():
		print("OK")
		quit(0)
		return
	for failure: String in _failures:
		printerr("FAIL %s" % failure)
	quit(1)
