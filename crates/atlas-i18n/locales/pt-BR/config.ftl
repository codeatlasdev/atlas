config-lang-current = Idioma de exibição: { $lang }

config-lang-source = { $source ->
    [env] Origem: a variável de ambiente ATLAS_LANG
    [file] Origem: a preferência salva (~/.atlas/language)
    [posix] Origem: a variável de ambiente LC_ALL, LC_MESSAGES ou LANG
    [os] Origem: o idioma do sistema
   *[default] Origem: nenhuma definida, usando inglês
}

config-lang-saved = Preferência salva: { $lang }

config-lang-saved-none = Preferência salva: nenhuma, o Atlas segue o idioma do sistema

config-lang-available = { $count ->
    [one] Idioma disponível: { $langs }
   *[other] Idiomas disponíveis: { $langs }
}

config-lang-set = Idioma de exibição definido como { $lang }.

config-lang-set-env-warning = ATLAS_LANG={ $value } está definida neste shell e tem prioridade sobre a preferência salva.

config-lang-unset = Preferência removida. O Atlas segue o idioma do sistema.

config-lang-unsupported = Idioma não suportado: "{ $tag }". Disponíveis: { $langs }
