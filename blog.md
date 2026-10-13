---
layout: default
title: Devlog
permalink: /blog/
description: "Development notes on Verdandi's procedural composition, synthesized instruments, Bevy integration, and audio quality."
---

# Verdandi devlog

Notes from building Verdandi: musical ideas, synthesis experiments, integration details, sound-quality improvements, and the occasional bug that needed a closer listen.

The project is evolving in public. The source code and roadmap are the authoritative places to check current behavior and planned work; these posts record the story behind changes as the engine grows.

Read the [repository](https://github.com/right-ward/verdandi) or [development roadmap](https://github.com/right-ward/verdandi/blob/main/ROADMAP.md), and subscribe with the <a href="{{ '/feed.xml' | relative_url }}">Atom feed</a>.

{% assign blog_posts = site.posts %}
{% if blog_posts.size > 0 %}
<ul>
{% for post in blog_posts %}
  <li>
    <time datetime="{{ post.date | date_to_xmlschema }}">{{ post.date | date: "%B %-d, %Y" }}</time> — <a href="{{ post.url | relative_url }}">{{ post.title | escape }}</a>
    {% if post.excerpt %}
    <p>{{ post.excerpt | strip_html | normalize_whitespace | truncate: 240 | escape }}</p>
    {% endif %}
  </li>
{% endfor %}
</ul>
{% else %}
<p>No devlog entries have been published on this site yet. In the meantime, the README and roadmap document the current features and planned audio-quality work.</p>
{% endif %}
