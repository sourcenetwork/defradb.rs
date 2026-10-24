// Mermaid bootstrap for the Book of Defra.
// Renders diagrams with a palette matched to the book's ayu theme and the
// diagram colour conventions (blue engine, green database, purple traits,
// orange races, yellow gates, grey boundaries).

(() => {
    const darkThemes = ['ayu', 'navy', 'coal'];
    const lightThemes = ['light', 'rust'];

    const classList = document.getElementsByTagName('html')[0].classList;

    let lastThemeWasLight = true;
    for (const cssClass of classList) {
        if (darkThemes.includes(cssClass)) {
            lastThemeWasLight = false;
            break;
        }
    }

    const lightVariables = {
        primaryColor: '#d6e6f8',
        primaryBorderColor: '#1565c0',
        primaryTextColor: '#1f2328',
        lineColor: '#57606a',
        textColor: '#1f2328',
        mainBkg: '#f6f8fa',
        nodeBorder: '#57606a',
        clusterBkg: '#f0f4f9',
        clusterBorder: '#c3d0dd',
        defaultLinkColor: '#57606a',
        titleColor: '#1f2328',
        edgeLabelBackground: '#ffffff',
        actorBkg: '#d6e6f8',
        actorBorder: '#1565c0',
        actorTextColor: '#1f2328',
        actorLineColor: '#8a97a5',
        signalColor: '#1f2328',
        signalTextColor: '#1f2328',
        labelBoxBkg: '#f3e5f5',
        labelBoxBorderColor: '#7b1fa2',
        labelTextColor: '#1f2328',
        loopTextColor: '#1f2328',
        noteBkgColor: '#fff3cd',
        noteBorderColor: '#b8860b',
        noteTextColor: '#1f2328',
        activationBorderColor: '#2e7d32',
        activationBkgColor: '#d8f0d8',
        sequenceNumberColor: '#ffffff',
        altBackground: '#f6f8fa',
    };

    const darkVariables = {
        background: '#0b0e14',
        primaryColor: '#0f3244',
        primaryBorderColor: '#10cbff',
        primaryTextColor: '#e6e1cf',
        lineColor: '#9aa4b2',
        textColor: '#e6e1cf',
        mainBkg: '#1a1f29',
        nodeBorder: '#9aa4b2',
        clusterBkg: '#131720',
        clusterBorder: '#2d333b',
        defaultLinkColor: '#9aa4b2',
        titleColor: '#00c951',
        edgeLabelBackground: '#1a1f29',
        actorBkg: '#0f3244',
        actorBorder: '#10cbff',
        actorTextColor: '#e6e1cf',
        actorLineColor: '#4c5661',
        signalColor: '#e6e1cf',
        signalTextColor: '#e6e1cf',
        labelBoxBkg: '#372a4d',
        labelBoxBorderColor: '#b794f6',
        labelTextColor: '#e6e1cf',
        loopTextColor: '#00c951',
        noteBkgColor: '#463a10',
        noteBorderColor: '#ffe066',
        noteTextColor: '#e6e1cf',
        activationBorderColor: '#06b250',
        activationBkgColor: '#0f2e1e',
        sequenceNumberColor: '#0b0e14',
        altBackground: '#131720',
    };

    mermaid.initialize({
        startOnLoad: true,
        theme: 'base',
        themeVariables: lastThemeWasLight ? lightVariables : darkVariables,
        flowchart: { curve: 'basis', htmlLabels: true },
        sequence: { actorMargin: 48, boxMargin: 10, useMaxWidth: true },
        gantt: { useMaxWidth: true },
    });

    // Simplest way to make mermaid re-render the diagrams in the new theme is via refreshing the page

    for (const darkTheme of darkThemes) {
        document.getElementById('mdbook-theme-' + darkTheme).addEventListener('click', () => {
            if (lastThemeWasLight) {
                window.location.reload();
            }
        });
    }

    for (const lightTheme of lightThemes) {
        document.getElementById('mdbook-theme-' + lightTheme).addEventListener('click', () => {
            if (!lastThemeWasLight) {
                window.location.reload();
            }
        });
    }
})();
