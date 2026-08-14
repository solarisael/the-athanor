extends Control

const PHASE_SHADER: Shader = preload("res://effects_lab/phase_treatment.gdshader")
const PHASE_STOKE: Resource = preload("res://effects_lab/phase_stoke.tres")
const JUICEE_EFFECT: Script = preload("res://addons/juicee/effects/juicee_effect.gd")

const GOLD := Color("c2a574")
const TEXT := Color("dfdacd")
const MUTED := Color("b3ada4")
const DARK := Color("050609")
const CARD_GAP := 16.0
const PHASES := [
	{
		"name": "NIGREDO",
		"title": "Black work",
		"body": "Particulate soot, erosion and low fog. The signal is decomposition, never failure authority.",
		"a": Color("090a0d"), "b": Color("373c48"), "accent": Color("7b828f")
	},
	{
		"name": "ALBEDO",
		"title": "White work",
		"body": "Cool glass, reflection and clarified emission. The signal is legibility, never verified truth.",
		"a": Color("101720"), "b": Color("b8cadb"), "accent": Color("eef8ff")
	},
	{
		"name": "CITRINITAS",
		"title": "Yellow work",
		"body": "Archive light, slow rings and gold dust. The signal is discovered relation, never Host evidence.",
		"a": Color("21150b"), "b": Color("bd743c"), "accent": Color("f6cf78")
	},
	{
		"name": "RUBEDO",
		"title": "Red work",
		"body": "Heat, embers and resolved-path emission. The signal is completion theatre, never a completion receipt.",
		"a": Color("23090b"), "b": Color("b73635"), "accent": Color("ff9b70")
	}
]

var _cards: Array[Control] = []
var _phase_materials: Array[ShaderMaterial] = []
var _runs: Array[Resource] = []
var _selected_phase := 0
var _reduced_motion := false
var _no_flash := true
var _no_chromatic := false
var _metrics_elapsed := 0.0
var _viewport_rid: RID

var _flow: HFlowContainer
var _page_margin: MarginContainer
var _status: Label
var _metrics: Label
var _viewport_value: Label
var _reduced_toggle: CheckButton
var _flash_toggle: CheckButton
var _chromatic_toggle: CheckButton

func _ready() -> void:
	_build_surface()
	_viewport_rid = get_viewport().get_viewport_rid()
	RenderingServer.viewport_set_measure_render_time(_viewport_rid, true)
	get_viewport().size_changed.connect(_on_viewport_size_changed)
	_apply_accessibility()
	call_deferred("_apply_responsive_layout")
	_refresh_metrics()

func _exit_tree() -> void:
	cancel_all_runs()
	if _viewport_rid.is_valid():
		RenderingServer.viewport_set_measure_render_time(_viewport_rid, false)

func _process(delta: float) -> void:
	_metrics_elapsed += delta
	if _metrics_elapsed < 0.5:
		return
	_metrics_elapsed = 0.0
	_refresh_metrics()

func _build_surface() -> void:
	var ground := Panel.new()
	ground.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	ground.theme_type_variation = &"AthanorGround"
	ground.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(ground)

	var scroll := ScrollContainer.new()
	scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	add_child(scroll)

	_page_margin = MarginContainer.new()
	_page_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_page_margin.add_theme_constant_override("margin_left", 48)
	_page_margin.add_theme_constant_override("margin_top", 32)
	_page_margin.add_theme_constant_override("margin_right", 48)
	_page_margin.add_theme_constant_override("margin_bottom", 40)
	scroll.add_child(_page_margin)

	var column := VBoxContainer.new()
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.add_theme_constant_override("separation", 18)
	_page_margin.add_child(column)

	var kicker := Label.new()
	kicker.text = "THE ATHANOR / DURABLE EFFECTS LABORATORY"
	kicker.theme_type_variation = &"AthanorPageKicker"
	column.add_child(kicker)

	var title := Label.new()
	title.text = "Four phase treatments, one honest boundary."
	title.theme_type_variation = &"AthanorTitle"
	title.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(title)

	var disclosure := Label.new()
	disclosure.text = "Decorative rendering only. No treatment, animation, color or particle carries Host authority, verification state, routing truth or completion evidence."
	disclosure.theme_type_variation = &"AthanorBody"
	disclosure.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	disclosure.modulate = Color("e8c58b")
	column.add_child(disclosure)

	var controls := HFlowContainer.new()
	controls.add_theme_constant_override("h_separation", 10)
	controls.add_theme_constant_override("v_separation", 8)
	column.add_child(controls)

	controls.add_child(_make_action("STOKE SELECTED", play_selected_phase))
	controls.add_child(_make_action("OVERLAP ×3", play_overlap_contract))
	controls.add_child(_make_action("CANCEL + RESTORE", cancel_all_runs))
	controls.add_child(_make_action("RESET", reset_lab_state))

	_reduced_toggle = _make_toggle("REDUCED MOTION", false, _on_reduced_motion_toggled)
	_flash_toggle = _make_toggle("NO FLASH", true, _on_no_flash_toggled)
	_chromatic_toggle = _make_toggle("NO CHROMATIC", false, _on_no_chromatic_toggled)
	controls.add_child(_reduced_toggle)
	controls.add_child(_flash_toggle)
	controls.add_child(_chromatic_toggle)

	_status = Label.new()
	_status.text = "Ready. Nigredo selected. Flash blocked by default."
	_status.theme_type_variation = &"AthanorCaption"
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(_status)

	_flow = HFlowContainer.new()
	_flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_flow.add_theme_constant_override("h_separation", int(CARD_GAP))
	_flow.add_theme_constant_override("v_separation", int(CARD_GAP))
	column.add_child(_flow)

	for index in PHASES.size():
		var card := _make_phase_card(index)
		_cards.append(card)
		_flow.add_child(card)
	_update_selection()

	var instrumentation := PanelContainer.new()
	instrumentation.theme_type_variation = &"AthanorVessel"
	column.add_child(instrumentation)
	var metrics_margin := MarginContainer.new()
	metrics_margin.add_theme_constant_override("margin_left", 18)
	metrics_margin.add_theme_constant_override("margin_top", 14)
	metrics_margin.add_theme_constant_override("margin_right", 18)
	metrics_margin.add_theme_constant_override("margin_bottom", 14)
	instrumentation.add_child(metrics_margin)
	var metrics_column := VBoxContainer.new()
	metrics_column.add_theme_constant_override("separation", 7)
	metrics_margin.add_child(metrics_column)
	var metrics_kicker := Label.new()
	metrics_kicker.text = "LIVE RENDER INSTRUMENTATION · 500 MS SAMPLE"
	metrics_kicker.theme_type_variation = &"AthanorKicker"
	metrics_column.add_child(metrics_kicker)
	_metrics = Label.new()
	_metrics.theme_type_variation = &"AthanorCaption"
	_metrics.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	metrics_column.add_child(_metrics)
	_viewport_value = Label.new()
	_viewport_value.theme_type_variation = &"AthanorMeta"
	_viewport_value.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	metrics_column.add_child(_viewport_value)

	var provenance := Label.new()
	provenance.text = "ORCHESTRATION  Juicee 1.4.2 · commit 13b0885 · exact tree e2bf326c · MIT · editor plugin/updater disabled · real phase_stoke.tres runtime resource"
	provenance.theme_type_variation = &"AthanorMeta"
	provenance.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(provenance)

func _make_action(label_text: String, callback: Callable) -> Button:
	var button := Button.new()
	button.text = label_text
	button.theme_type_variation = &"AthanorTab"
	button.pressed.connect(callback)
	return button

func _make_toggle(label_text: String, initial: bool, callback: Callable) -> CheckButton:
	var toggle := CheckButton.new()
	toggle.text = label_text
	toggle.button_pressed = initial
	toggle.theme_type_variation = &"AthanorTab"
	toggle.toggled.connect(callback)
	return toggle

func _make_phase_card(index: int) -> Control:
	var phase: Dictionary = PHASES[index]
	var card := Control.new()
	card.name = "PhaseCard%d" % index
	card.custom_minimum_size = Vector2(260, 248)
	card.clip_contents = true
	card.tooltip_text = "%s treatment. Decorative only." % phase.name

	var background := ColorRect.new()
	background.name = "Treatment"
	background.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	background.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var material := ShaderMaterial.new()
	material.shader = PHASE_SHADER
	material.set_shader_parameter("phase", index)
	material.set_shader_parameter("tone_a", phase.a)
	material.set_shader_parameter("tone_b", phase.b)
	material.set_shader_parameter("accent", phase.accent)
	background.material = material
	_phase_materials.append(material)
	card.add_child(background)
	var scrim := ColorRect.new()
	scrim.name = "LegibilityScrim"
	scrim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	scrim.color = Color(0.005, 0.006, 0.009, 0.42)
	scrim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(scrim)

	var border := Panel.new()
	border.name = "SelectionBorder"
	border.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	border.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(border)

	var content := MarginContainer.new()
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	content.add_theme_constant_override("margin_left", 18)
	content.add_theme_constant_override("margin_top", 16)
	content.add_theme_constant_override("margin_right", 18)
	content.add_theme_constant_override("margin_bottom", 16)
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(content)
	var stack := VBoxContainer.new()
	stack.add_theme_constant_override("separation", 7)
	stack.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_child(stack)

	var phase_label := Label.new()
	phase_label.text = phase.name
	phase_label.theme_type_variation = &"AthanorKicker"
	phase_label.modulate = phase.accent
	phase_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(phase_label)
	var heading := Label.new()
	heading.text = phase.title
	heading.theme_type_variation = &"AthanorHeading"
	heading.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(heading)
	var source := Label.new()
	source.text = "SOURCE · GODOT CANVASITEM SHADER"
	source.theme_type_variation = &"AthanorMeta"
	source.modulate = Color("f2dfbd")
	source.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(source)
	var body := Label.new()
	body.text = phase.body
	body.theme_type_variation = &"AthanorCaption"
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(body)
	var boundary := Label.new()
	boundary.text = "DECORATIVE · ZERO HOST AUTHORITY"
	boundary.theme_type_variation = &"AthanorMeta"
	boundary.modulate = Color("e8c58b")
	boundary.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(boundary)

	var hit_target := Button.new()
	hit_target.name = "Select"
	hit_target.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	hit_target.flat = true
	hit_target.tooltip_text = "Select %s" % phase.name.capitalize()
	hit_target.accessibility_name = "Select %s phase treatment" % phase.name.capitalize()
	hit_target.pressed.connect(_select_phase.bind(index))
	card.add_child(hit_target)
	return card

func _select_phase(index: int) -> void:
	_selected_phase = index
	_update_selection()
	_status.text = "%s selected. Ready to stoke." % PHASES[index].name.capitalize()

func _update_selection() -> void:
	for index in _cards.size():
		var border := _cards[index].get_node("SelectionBorder") as Panel
		var style := StyleBoxFlat.new()
		style.bg_color = Color.TRANSPARENT
		style.set_border_width_all(2 if index == _selected_phase else 1)
		style.border_color = GOLD if index == _selected_phase else Color(0.47, 0.45, 0.43, 0.40)
		style.corner_radius_top_left = 5
		style.corner_radius_top_right = 5
		style.corner_radius_bottom_left = 5
		style.corner_radius_bottom_right = 5
		border.add_theme_stylebox_override("panel", style)

func play_selected_phase() -> void:
	_start_run(_cards[_selected_phase])
	_status.text = "%s stoked through the pinned phase_stoke.tres sequence." % PHASES[_selected_phase].name.capitalize()

func play_overlap_contract() -> void:
	var card := _cards[_selected_phase]
	# One sequence owns the BackBufferCopy lane; the other two still overlap every
	# property effect. Competing screen samplers are deliberately not composited.
	_start_run(card, true)
	_start_run(card, false)
	_start_run(card, false)
	_status.text = "Three independent sequence instances overlap on %s. One owns the backbuffer; cancel must restore every captured property." % PHASES[_selected_phase].name.capitalize()

func _start_run(card: Control, include_screen_effects: bool = true) -> void:
	var run: Resource = PHASE_STOKE.duplicate(true)
	for effect in run.effects.duplicate():
		var script_path: String = effect.get_script().resource_path
		var is_scale := script_path.ends_with("/scale_effect.gd")
		var is_screen_effect := script_path.ends_with("/chromatic_effect.gd") or script_path.ends_with("/flash_effect.gd")
		if (_reduced_motion and is_scale) or (not include_screen_effects and is_screen_effect):
			run.effects.erase(effect)
	_runs.append(run)
	run.finished.connect(_retire_run.bind(run), CONNECT_ONE_SHOT)
	run.stopped.connect(_retire_run.bind(run), CONNECT_ONE_SHOT)
	run.play(card)

func _retire_run(run: Resource) -> void:
	_runs.erase(run)

func cancel_all_runs() -> void:
	var active := _runs.duplicate()
	for run in active:
		run.stop()
	_runs.clear()
	_status.text = "All active sequences cancelled. Juicee state stacks released; overlays swept."

func reset_lab_state() -> void:
	cancel_all_runs()
	_selected_phase = 0
	_reduced_toggle.button_pressed = false
	_flash_toggle.button_pressed = true
	_chromatic_toggle.button_pressed = false
	for card in _cards:
		card.modulate = Color.WHITE
		card.scale = Vector2.ONE
	_update_selection()
	_status.text = "Laboratory reset. Nigredo selected. Flash blocked by default."

func _on_reduced_motion_toggled(enabled: bool) -> void:
	_reduced_motion = enabled
	_apply_accessibility()

func _on_no_flash_toggled(enabled: bool) -> void:
	_no_flash = enabled
	_apply_accessibility()

func _on_no_chromatic_toggled(enabled: bool) -> void:
	_no_chromatic = enabled
	_apply_accessibility()

func _apply_accessibility() -> void:
	JUICEE_EFFECT.accessibility.reduced_motion = _reduced_motion
	JUICEE_EFFECT.accessibility.no_flash = _no_flash
	JUICEE_EFFECT.accessibility.no_chromatic = _no_chromatic
	for material in _phase_materials:
		material.set_shader_parameter("motion_scale", 0.0 if _reduced_motion else 1.0)
		material.set_shader_parameter("chromatic_strength", 0.0 if _no_chromatic else 0.18)
	if _status:
		_status.text = "Accessibility · reduced motion %s · no flash %s · no chromatic %s" % [
			_on_off(_reduced_motion), _on_off(_no_flash), _on_off(_no_chromatic)
		]

func _on_viewport_size_changed() -> void:
	_apply_responsive_layout()
	_refresh_metrics()

func _apply_responsive_layout() -> void:
	if not _flow:
		return
	var viewport_width := float(get_viewport_rect().size.x)
	_page_margin.custom_minimum_size.x = viewport_width
	var usable := maxf(240.0, viewport_width - 96.0)
	var columns := 4
	if viewport_width < 760.0:
		columns = 1
	elif viewport_width < 1260.0:
		columns = 2
	var card_width: float = floorf((usable - CARD_GAP * float(columns - 1)) / float(columns))
	for card in _cards:
		card.custom_minimum_size.x = card_width
		card.size.x = card_width

func _refresh_metrics() -> void:
	if not _metrics or not _viewport_value:
		return
	var fps := Performance.get_monitor(Performance.TIME_FPS)
	var cpu_ms := RenderingServer.viewport_get_measured_render_time_cpu(_viewport_rid)
	var gpu_ms := RenderingServer.viewport_get_measured_render_time_gpu(_viewport_rid)
	var draw_calls := int(Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME))
	var objects := int(Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME))
	var video_memory := Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED)
	_metrics.text = "FPS  %.0f     CPU RENDER  %.3f ms     GPU RENDER  %.3f ms     DRAW CALLS  %d     OBJECTS  %d     VIDEO MEMORY  %s" % [
		fps, cpu_ms, gpu_ms, draw_calls, objects, _format_mib(video_memory)
	]
	var viewport_size := get_viewport_rect().size
	_viewport_value.text = "VIEWPORT  %d × %d     RENDERER  %s / %s     ADAPTER  %s     ACTIVE RUNS  %d" % [
		int(viewport_size.x), int(viewport_size.y), RenderingServer.get_current_rendering_method(),
		RenderingServer.get_current_rendering_driver_name(), RenderingServer.get_video_adapter_name(), _runs.size()
	]

func _format_mib(bytes: float) -> String:
	return "%.1f MiB" % (bytes / 1048576.0)

func _on_off(value: bool) -> String:
	return "ON" if value else "OFF"

# Behavioral-contract surface used by the headless runner. These methods exercise
# the same resource, cancellation and layout paths as the visible controls.
func contract_set_accessibility(reduced_motion: bool, no_flash: bool, no_chromatic: bool) -> void:
	_reduced_toggle.button_pressed = reduced_motion
	_flash_toggle.button_pressed = no_flash
	_chromatic_toggle.button_pressed = no_chromatic

func contract_cards() -> Array[Control]:
	return _cards

func contract_active_runs() -> int:
	return _runs.size()

func contract_overlay_count(layer_name: StringName) -> int:
	return _count_named_descendants(self, layer_name)

func contract_apply_layout() -> void:
	_apply_responsive_layout()

func _count_named_descendants(node: Node, sought: StringName) -> int:
	var count := 1 if node.name == sought else 0
	for child in node.get_children():
		count += _count_named_descendants(child, sought)
	return count
