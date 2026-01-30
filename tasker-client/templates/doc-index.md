{%- extends "base.md" -%}

{% block title %}# Tasker Configuration Documentation Index{% endblock -%}

{% block content %}

> **Coverage:** {{ documented_parameters }}/{{ total_parameters }} parameters documented ({{ coverage_percent }}%)

---

## Common Configuration
{% for section in common_sections -%}
- **{{ section.name }}** (`{{ section.path }}`) — {{ section.total_parameters() }} params
  {%- if section.documented_parameters() > 0 %} ({{ section.documented_parameters() }} documented){% endif %}
{% for sub in section.subsections -%}
  - {{ sub.name }} (`{{ sub.path }}`) — {{ sub.total_parameters() }} params
{% endfor -%}
{% endfor %}
## Orchestration Configuration
{% for section in orchestration_sections -%}
- **{{ section.name }}** (`{{ section.path }}`) — {{ section.total_parameters() }} params
  {%- if section.documented_parameters() > 0 %} ({{ section.documented_parameters() }} documented){% endif %}
{% for sub in section.subsections -%}
  - {{ sub.name }} (`{{ sub.path }}`) — {{ sub.total_parameters() }} params
{% endfor -%}
{% endfor %}
## Worker Configuration
{% for section in worker_sections -%}
- **{{ section.name }}** (`{{ section.path }}`) — {{ section.total_parameters() }} params
  {%- if section.documented_parameters() > 0 %} ({{ section.documented_parameters() }} documented){% endif %}
{% for sub in section.subsections -%}
  - {{ sub.name }} (`{{ sub.path }}`) — {{ sub.total_parameters() }} params
{% endfor -%}
{% endfor -%}
{% endblock %}
